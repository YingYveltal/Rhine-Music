use crate::library::{atomic_json, hash, timestamp};
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, Receiver};
use serde_json::{json, Value};
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex}};

pub type Response = Receiver<Result<Value, String>>;
pub trait Native: Send + Sync { fn call(&self, operation: &str, body: Value) -> Response; }
pub struct MusicKit;
#[cfg(target_os = "macos")]
extern "C" {
    fn rhine_apple_request(json: *const std::ffi::c_char, context: *mut std::ffi::c_void,
        callback: extern "C" fn(*mut std::ffi::c_void, *const std::ffi::c_char));
}
#[cfg(target_os = "macos")]
extern "C" fn reply(context: *mut std::ffi::c_void, raw: *const std::ffi::c_char) {
    // Each native request owns exactly one callback; no borrowed Rust state crosses await.
    let sender = unsafe { Box::from_raw(context.cast::<crossbeam_channel::Sender<Result<Value, String>>>()) };
    let result = (|| {
        if raw.is_null() { return Err("Apple Music 返回空响应".into()); }
        let bytes = unsafe { std::ffi::CStr::from_ptr(raw) }.to_bytes();
        let v: Value = serde_json::from_slice(bytes).map_err(|_| "Apple Music 响应格式错误".to_string())?;
        if v["ok"] == true { Ok(v["value"].clone()) } else { Err(v["error"].as_str().unwrap_or("Apple Music 操作失败").to_owned()) }
    })();
    let _ = sender.send(result);
}
impl Native for MusicKit {
    fn call(&self, operation: &str, body: Value) -> Response {
        let (tx, rx) = bounded(1);
        #[cfg(target_os = "macos")]
        {
            let request = std::ffi::CString::new(json!({"operation":operation,"body":body}).to_string()).unwrap();
            unsafe { rhine_apple_request(request.as_ptr(), Box::into_raw(Box::new(tx)).cast(), reply) };
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = body;
            let result = if operation == "status" { Ok(json!({"supported":false,"authorization":"notDetermined"})) }
                else { Err("Apple Music 功能需要 macOS 14 或更新版本".into()) };
            let _ = tx.send(result);
        }
        rx
    }
}
fn receive(rx: Response) -> Result<Value> { rx.recv().context("Apple Music 连接已结束")?.map_err(anyhow::Error::msg) }

struct Cache { albums: Vec<Value>, enabled: bool, updated_at: Option<String>, status: Value }
pub struct Apple { dir: PathBuf, cache: Mutex<Cache>, pub native: Arc<dyn Native> }
impl Apple {
    pub fn new(dir: &Path) -> Result<Arc<Self>> {
        let dir = dir.join("apple"); std::fs::create_dir_all(dir.join("covers"))?;
        let saved: Value = std::fs::read(dir.join("library.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(json!({}));
        let enabled = saved["enabled"].as_bool().unwrap_or(true);
        let albums = saved["albums"].as_array().cloned().unwrap_or_default();
        let updated_at = saved["updatedAt"].as_str().map(str::to_owned);
        Ok(Arc::new(Self { dir, native: Arc::new(MusicKit), cache: Mutex::new(Cache { albums, enabled, updated_at,
            status: json!({"supported":cfg!(target_os="macos"),"authorization":"notDetermined", "unavailableReason":null,
                "job":{"running":false,"completed":0,"total":0,"message":null,"error":null}}) }) }))
    }
    pub fn status(&self) -> Value {
        let c = self.cache.lock().unwrap(); let mut s = c.status.clone();
        s["enabled"] = json!(c.enabled); s["updatedAt"] = json!(c.updated_at);
        s["playlistCount"] = json!(c.albums.len());
        s["trackCount"] = json!(c.albums.iter().map(|a|a["tracks"].as_array().map_or(0,Vec::len)).sum::<usize>());
        s
    }
    pub fn albums(&self) -> Vec<Value> {
        let c=self.cache.lock().unwrap(); if c.enabled { c.albums.clone() } else { vec![] }
    }
    pub fn cover_root(&self) -> PathBuf { self.dir.join("covers") }
    pub fn playable(&self) -> Result<()> {
        let c=self.cache.lock().unwrap();
        anyhow::ensure!(c.enabled,"请先启用 Apple Music");
        anyhow::ensure!(c.status["supported"]==true,"Apple Music 需要 macOS 14 或更新版本");
        Ok(())
    }
    fn save(&self, c: &Cache) -> Result<()> {
        atomic_json(&self.dir.join("library.json"), &json!({"albums":c.albums,"enabled":c.enabled,"updatedAt":c.updated_at}))
    }
    pub fn request(self: &Arc<Self>, op: &str, body: Value) -> Result<Value> {
        match op {
            "status" | "authorize" => {
                let value = receive(self.native.call(op,json!({})))?;
                let mut c=self.cache.lock().unwrap();
                for key in ["supported","authorization"] { if let Some(v)=value.get(key) { c.status[key]=v.clone(); } }
                if c.status["supported"]==false { c.status["unavailableReason"]=json!("Apple Music 需要 macOS 14 或更新版本"); }
                if c.status["job"]["running"]==true {
                    if let Some(v)=value.get("completed") { c.status["job"]["completed"]=v.clone(); }
                    if let Some(v)=value.get("total") { c.status["job"]["total"]=v.clone(); }
                }
            }
            "enable" => {
                let enabled=body["enabled"].as_bool().context("enabled 必须是布尔值")?;
                let mut c=self.cache.lock().unwrap(); let previous=c.enabled; c.enabled=enabled;
                if let Err(e)=self.save(&c) { c.enabled=previous; return Err(e); }
            }
            "sync" => {
                let status=receive(self.native.call("status",json!({})))?;
                anyhow::ensure!(status["supported"]==true,"Apple Music 需要 macOS 14 或更新版本");
                anyhow::ensure!(status["authorization"]=="authorized","请先允许访问音乐资料库");
                let mut c=self.cache.lock().unwrap();
                c.status["authorization"]=status["authorization"].clone();
                if c.status["job"]["running"]!=true {
                    c.status["job"]=json!({"running":true,"completed":0,"total":0,"message":"正在读取个人歌单","error":null});
                    let this=self.clone(); std::thread::spawn(move || this.sync());
                }
            }
            _ => bail!("不支持的 Apple Music 操作"),
        }
        Ok(self.status())
    }
    fn sync(&self) {
        let result=(|| -> Result<Vec<Value>> {
            let value=receive(self.native.call("sync",json!({})))?;
            let mut albums=value.as_array().context("歌单响应格式错误")?.clone();
            let client=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(12)).build()?;
            for album in &mut albums {
                if let Some(url)=album["artworkURL"].as_str() {
                    let path=self.cover_root().join(format!("{}.img",hash(url)));
                    if !path.is_file() {
                        // Artwork comes from MusicKit; cover failure must not discard playable songs.
                        if let Ok(response)=client.get(url).send().and_then(|r|r.error_for_status()) {
                            if let Ok(bytes)=response.bytes() { if bytes.len()<10_000_000 { let _=std::fs::write(&path,bytes); } }
                        }
                    }
                    if path.is_file() { album["nativeCoverPath"]=json!(path); }
                }
                if let Some(o)=album.as_object_mut(){o.remove("artworkURL");}
            }
            Ok(albums)
        })();
        let mut c=self.cache.lock().unwrap();
        match result {
            Ok(albums) => {
                let count=albums.len();
                let next=Cache {albums,enabled:c.enabled,updated_at:Some(timestamp()),status:c.status.clone()};
                match self.save(&next) {
                    Ok(()) => { *c=next;c.status["job"]=json!({"running":false,"completed":count,"total":count,"message":null,"error":null}); }
                    Err(e) => { c.status["job"]["running"]=json!(false);c.status["job"]["error"]=json!(e.to_string()); }
                }
            }
            Err(e) => { c.status["job"]["running"]=json!(false);c.status["job"]["error"]=json!(e.to_string()); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(Mutex<Option<Result<Value,String>>>);
    impl Native for Fake {
        fn call(&self, _: &str, _: Value) -> Response {
            let(tx,rx)=bounded(1);tx.send(self.0.lock().unwrap().take().unwrap()).unwrap();rx
        }
    }
    #[test]
    fn failed_sync_preserves_the_complete_snapshot_on_disk_and_in_memory() {
        let dir=tempfile::tempdir().unwrap();
        let mut apple=Apple::new(dir.path()).unwrap();
        let this=Arc::get_mut(&mut apple).unwrap();
        this.native=Arc::new(Fake(Mutex::new(Some(Err("page 2 failed".into())))));
        {let mut c=this.cache.lock().unwrap();c.enabled=true;c.albums=vec![json!({"id":"old","tracks":[{"id":"occurrence-1"},{"id":"occurrence-2"}]})];this.save(&c).unwrap();}
        let old=std::fs::read(this.dir.join("library.json")).unwrap();
        this.sync();
        assert_eq!(std::fs::read(this.dir.join("library.json")).unwrap(),old);
        assert_eq!(this.albums()[0]["tracks"].as_array().unwrap().len(),2);
        assert_eq!(this.status()["job"]["running"],false);
        assert_eq!(this.status()["job"]["error"],"page 2 failed");
    }
    #[test]
    fn enabling_and_restart_preserve_occurrence_order_without_music_metadata_deduplication() {
        let dir=tempfile::tempdir().unwrap();let mut apple=Apple::new(dir.path()).unwrap();
        let this=Arc::get_mut(&mut apple).unwrap();
        this.native=Arc::new(Fake(Mutex::new(Some(Ok(json!([{"id":"playlist","tracks":[
            {"id":"entry-1","title":"same"},{"id":"entry-2","title":"same"}]}]))))));
        this.sync();apple.request("enable",json!({"enabled":true})).unwrap();
        let reopened=Apple::new(dir.path()).unwrap();
        assert_eq!(reopened.albums()[0]["tracks"][0]["id"],"entry-1");
        assert_eq!(reopened.albums()[0]["tracks"][1]["id"],"entry-2");
        assert_eq!(reopened.status()["trackCount"],2);
        assert_eq!(reopened.status()["enabled"],true);
    }
    #[test]
    fn first_sync_is_visible_but_an_explicit_disabled_preference_survives_restart() {
        let dir=tempfile::tempdir().unwrap();let apple=Apple::new(dir.path()).unwrap();
        assert_eq!(apple.status()["enabled"],true);
        assert_eq!(apple.status()["authorization"],"notDetermined");
        assert_eq!(apple.status()["playlistCount"],0);
        apple.request("enable",json!({"enabled":false})).unwrap();
        assert_eq!(Apple::new(dir.path()).unwrap().status()["enabled"],false);
    }
}
