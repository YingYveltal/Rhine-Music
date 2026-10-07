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
    fn rhine_apple_artwork_valid(bytes: *const u8, count: usize) -> u8;
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

fn valid_artwork(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() >= 10_000_000 { return false; }
    #[cfg(target_os = "macos")]
    { unsafe { rhine_apple_artwork_valid(bytes.as_ptr(), bytes.len()) != 0 } }
    #[cfg(not(target_os = "macos"))]
    { false }
}
// A failed cover is optional metadata: try the first song, then keep the
// existing placeholder. Never publish an invalid or partially written file.
fn cache_artwork(album: &mut Value, root: &Path, mut fetch: impl FnMut(&str) -> Result<Vec<u8>>) {
    let urls: Vec<String> = ["artworkURL", "firstTrackArtworkURL"].iter()
        .filter_map(|key| album[*key].as_str().filter(|url| !url.is_empty()).map(str::to_owned)).collect();
    let Some(object) = album.as_object_mut() else { return; };
    object.remove("nativeCoverPath");
    object.remove("artworkURL");
    object.remove("firstTrackArtworkURL");
    let mut previous = None;
    for url in urls {
        if previous.as_ref() == Some(&url) { continue; }
        let path = root.join(format!("{}.img", hash(&url)));
        previous = Some(url.clone());
        let cached = std::fs::read(&path).ok().is_some_and(|bytes| valid_artwork(&bytes));
        let available = cached || (|| -> Result<bool> {
            let bytes = fetch(&url)?;
            if !valid_artwork(&bytes) { return Ok(false); }
            let temporary = path.with_extension("tmp");
            std::fs::write(&temporary, bytes)?;
            std::fs::rename(&temporary, &path)?;
            Ok(true)
        })().unwrap_or(false);
        if available { object.insert("nativeCoverPath".into(), json!(path)); break; }
    }
}

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
        s["playlistCount"] = json!(c.albums.iter().filter(|a| a["kind"] != "album").count());
        s["albumCount"] = json!(c.albums.iter().filter(|a| a["kind"] == "album").count());
        s["unconfirmedPlaylistCount"] = json!(c.albums.iter().filter(|a| a["kind"] != "album" && a["complete"] == false).count());
        s["unconfirmedAlbumCount"] = json!(c.albums.iter().filter(|a| a["kind"] == "album" && a["complete"] == false).count());
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
                    c.status["job"]=json!({"running":true,"completed":0,"total":0,"message":"正在读取专辑和歌单","error":null});
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
            let mut albums=value.as_array().context("资料库响应格式错误")?.clone();
            let client=reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(12)).build()?;
            for album in &mut albums {
                cache_artwork(album, &self.cover_root(), |url| {
                    if reqwest::Url::parse(url)?.scheme() == "musickit" {
                        use base64::Engine;
                        let value = receive(self.native.call("artwork", json!({"url":url})))?;
                        let encoded = value["data"].as_str().context("封面响应格式错误")?;
                        return Ok(base64::engine::general_purpose::STANDARD.decode(encoded)?);
                    }
                    let mut response = client.get(url).send()?.error_for_status()?;
                    let mut bytes = Vec::new();
                    use std::io::Read;
                    response.by_ref().take(10_000_000).read_to_end(&mut bytes)?;
                    Ok(bytes)
                });
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
    fn artwork_png() -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGNw6TgDAAKsAZkaoN/PAAAAAElFTkSuQmCC").unwrap()
    }
    #[test]
    fn playlist_artwork_wins_without_fetching_first_song_and_reuses_valid_cache() {
        let dir=tempfile::tempdir().unwrap();
        let mut album=json!({"artworkURL":"playlist","firstTrackArtworkURL":"first","tracks":[{"id":"first"}]});
        let mut fetched=vec![];
        cache_artwork(&mut album,dir.path(),|url|{fetched.push(url.to_owned());Ok(artwork_png())});
        assert_eq!(fetched,vec!["playlist"]);
        assert_eq!(album["nativeCoverPath"],json!(dir.path().join(format!("{}.img",hash("playlist")))));
        assert!(album.get("artworkURL").is_none() && album.get("firstTrackArtworkURL").is_none());
        let mut again=json!({"artworkURL":"playlist","firstTrackArtworkURL":"first"});
        cache_artwork(&mut again,dir.path(),|_|panic!("valid cached image must not be downloaded again"));
        assert_eq!(again["nativeCoverPath"],album["nativeCoverPath"]);
    }
    #[test]
    fn missing_playlist_artwork_uses_only_the_provided_first_song() {
        let dir=tempfile::tempdir().unwrap();
        let mut album=json!({"firstTrackArtworkURL":"first","tracks":[{"id":"first"},{"id":"second"}]});
        let tracks=album["tracks"].clone();
        cache_artwork(&mut album,dir.path(),|url|{assert_eq!(url,"first");Ok(artwork_png())});
        assert_eq!(album["nativeCoverPath"],json!(dir.path().join(format!("{}.img",hash("first")))));
        assert_eq!(album["tracks"],tracks);
    }
    #[test]
    fn failed_or_invalid_original_artwork_falls_back_and_rejects_corrupt_cache() {
        for failure in ["network","html","empty"] {
            let dir=tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(format!("{}.img",hash("playlist"))),b"broken cached image").unwrap();
            let mut album=json!({"artworkURL":"playlist","firstTrackArtworkURL":"first"});
            let mut fetched=vec![];
            cache_artwork(&mut album,dir.path(),|url| {
                fetched.push(url.to_owned());
                if url=="first" { return Ok(artwork_png()); }
                match failure { "network"=>bail!("synthetic failure"), "html"=>Ok(b"<html>failure</html>".to_vec()), _=>Ok(vec![]) }
            });
            assert_eq!(fetched,vec!["playlist","first"]);
            assert_eq!(album["nativeCoverPath"],json!(dir.path().join(format!("{}.img",hash("first")))));
        }
    }
    #[test]
    fn absent_artwork_or_two_failures_leave_placeholder_and_playback_metadata_intact() {
        let dir=tempfile::tempdir().unwrap();
        for tracks in [json!([]),json!([{"id":"first","sourcePosition":0,"browserPlayable":false}])] {
            for candidates in [false,true] {
                let mut album=json!({"tracks":tracks,"snapshotRevision":"unchanged","nativeCoverPath":"stale"});
                if candidates {album["artworkURL"]=json!("playlist");album["firstTrackArtworkURL"]=json!("first");}
                cache_artwork(&mut album,dir.path(),|_|{assert!(candidates);bail!("synthetic failure")});
                assert!(album.get("nativeCoverPath").is_none());
                assert_eq!(album["tracks"],tracks);assert_eq!(album["snapshotRevision"],"unchanged");
            }
        }
    }
    #[test]
    fn fully_transparent_playlist_artwork_falls_back_but_opaque_solid_art_is_valid() {
        use base64::Engine;
        let transparent=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4b6zEAAAE3gFVTX0OowAAAABJRU5ErkJggg==").unwrap();
        assert!(!valid_artwork(&transparent));assert!(valid_artwork(&artwork_png()));
        let dir=tempfile::tempdir().unwrap();
        let mut album=json!({"artworkURL":"playlist","firstTrackArtworkURL":"first"});
        cache_artwork(&mut album,dir.path(),|url|Ok(if url=="playlist" {transparent.clone()} else {artwork_png()}));
        assert_eq!(album["nativeCoverPath"],json!(dir.path().join(format!("{}.img",hash("first")))));
    }
    #[test]
    fn artwork_cache_write_failure_is_nonfatal() {
        let dir=tempfile::tempdir().unwrap();let blocked=dir.path().join("not-a-directory");
        std::fs::write(&blocked,b"occupied").unwrap();
        let mut album=json!({"artworkURL":"playlist","firstTrackArtworkURL":"first","tracks":[{"id":"first"}]});
        cache_artwork(&mut album,&blocked,|_|Ok(artwork_png()));
        assert!(album.get("nativeCoverPath").is_none());assert_eq!(album["tracks"][0]["id"],"first");
    }
    struct Fake(Mutex<Option<Result<Value,String>>>);
    impl Native for Fake {
        fn call(&self, _: &str, _: Value) -> Response {
            let(tx,rx)=bounded(1);tx.send(self.0.lock().unwrap().take().unwrap()).unwrap();rx
        }
    }
    #[test]
    fn unconfirmed_empty_does_not_block_other_playlists_or_claim_a_complete_library() {
        let dir=tempfile::tempdir().unwrap();let mut apple=Apple::new(dir.path()).unwrap();
        let this=Arc::get_mut(&mut apple).unwrap();
        // Empty reads retain the normal snapshot replacement behavior. They
        // carry uncertainty explicitly, rather than silently claiming success.
        {let mut c=this.cache.lock().unwrap();c.albums=vec![json!({"id":"empty","complete":true,
            "tracks":[{"id":"old"}]})];this.save(&c).unwrap();}
        let albums=json!([
            {"id":"empty","complete":false,"loadError":"本次未读取到歌曲","tracks":[]},
            {"id":"populated","complete":true,"tracks":[{"id":"one"},{"id":"two"}]}]);
        this.native=Arc::new(Fake(Mutex::new(Some(Ok(albums.clone())))));
        this.sync();
        assert_eq!(this.albums(),albums.as_array().unwrap().clone());
        let status=this.status();assert_eq!(status["playlistCount"],2);
        assert_eq!(status["trackCount"],2);assert_eq!(status["unconfirmedPlaylistCount"],1);
        assert_eq!(status["job"]["running"],false);assert!(status["job"]["error"].is_null());
        let reopened=Apple::new(dir.path()).unwrap();
        assert_eq!(reopened.status()["unconfirmedPlaylistCount"],1);
        assert_eq!(reopened.albums(),this.albums());
        // A later populated response naturally removes the uncertainty.
        this.native=Arc::new(Fake(Mutex::new(Some(Ok(json!([
            {"id":"empty","complete":true,"tracks":[{"id":"restored"}]}
        ]))))));
        this.sync();assert_eq!(this.status()["unconfirmedPlaylistCount"],0);
        assert_eq!(this.albums()[0]["tracks"][0]["id"],"restored");
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
    fn mixed_library_counts_metadata_and_failure_preservation() {
        let dir=tempfile::tempdir().unwrap();let mut apple=Apple::new(dir.path()).unwrap();
        let this=Arc::get_mut(&mut apple).unwrap();
        let collections=json!([
            {"id":"legacy-playlist","tracks":[{"id":"old-entry"}]},
            {"id":"empty-playlist","kind":"playlist","complete":false,"tracks":[]},
            {"id":"apple-album-one","kind":"album","complete":true,"year":2000,"tracks":[
                {"id":"apple-track-album-a","discNumber":1,"trackNumber":4},
                {"id":"apple-track-album-b","discNumber":2,"trackNumber":7}]},
            {"id":"apple-album-empty","kind":"album","complete":false,"tracks":[]}]);
        this.native=Arc::new(Fake(Mutex::new(Some(Ok(collections.clone())))));this.sync();
        let status=this.status();
        assert_eq!(status["playlistCount"],2);assert_eq!(status["albumCount"],2);
        assert_eq!(status["unconfirmedPlaylistCount"],1);assert_eq!(status["unconfirmedAlbumCount"],1);
        assert_eq!(status["trackCount"],3);
        assert_eq!(this.albums(),collections.as_array().unwrap().clone());
        assert_eq!(Apple::new(dir.path()).unwrap().albums(),this.albums());
        let before=std::fs::read(this.dir.join("library.json")).unwrap();
        this.native=Arc::new(Fake(Mutex::new(Some(Err("album page failed".into())))));this.sync();
        assert_eq!(std::fs::read(this.dir.join("library.json")).unwrap(),before);
        assert_eq!(this.albums(),collections.as_array().unwrap().clone());
        assert_eq!(this.status()["job"]["error"],"album page failed");
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
