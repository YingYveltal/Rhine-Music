use crate::library::{Album,Cover,Track,atomic_json,timestamp};
use anyhow::{bail,Context,Result};
use rhine_qq_connector::{web::{WebClient,audio_host},official::OfficialClient};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use std::{collections::{HashMap,HashSet},io::{Read,Write},path::{Path,PathBuf},sync::{Arc,Mutex,atomic::{AtomicU64,Ordering}},time::Duration};

#[derive(Clone,Default,Serialize,Deserialize)]
struct Song {mid:String,kind:u64}
#[derive(Clone,Default,Serialize,Deserialize)]
struct Saved {albums:Vec<Album>,songs:HashMap<String,Song>,updated:String,enabled:bool}
pub struct Qq {
    web:Mutex<WebClient>, key:Mutex<String>, saved:Mutex<Saved>, job:Mutex<Value>,
    root:PathBuf, generation:AtomicU64, remembered:std::sync::atomic::AtomicBool,
}
impl Qq {
    pub fn new(dir:&Path)->Result<Arc<Self>> {
        let root=dir.join("qq");std::fs::create_dir_all(root.join("audio"))?;std::fs::create_dir_all(root.join("covers"))?;
        let remembered=dir.join("qq/remember-connection").is_file();
        let stored=if remembered {security_framework::passwords::generic_password(security_framework::passwords::PasswordOptions::new_generic_password("com.rhine.music.qq", "connection")).ok().and_then(|v|serde_json::from_slice::<Value>(&v).ok())}else{None};
        let session=std::env::var("RHINE_QQ_SESSION").ok().and_then(|v|serde_json::from_str::<Value>(&v).ok()).or_else(||stored.as_ref().map(|v|v["session"].clone()));
        let key=std::env::var("QQMUSIC_API_KEY").ok().or_else(||stored.as_ref().and_then(|v|v["key"].as_str().map(str::to_owned))).unwrap_or_default();
        let saved=std::fs::read(root.join("library.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_default();
        let this=Arc::new(Self{web:Mutex::new(WebClient::new(session.as_ref())?),key:Mutex::new(key),saved:Mutex::new(saved),job:Mutex::new(json!({"running":false})),root,generation:AtomicU64::new(0),remembered:std::sync::atomic::AtomicBool::new(remembered)});
        std::env::remove_var("RHINE_QQ_SESSION");std::env::remove_var("QQMUSIC_API_KEY");
        Ok(this)
    }
    pub fn status(&self)->Value {
        let s=self.saved.lock().unwrap();let job=self.job.lock().unwrap().clone();
        json!({"remembered":self.remembered.load(Ordering::SeqCst),"connected":self.web.lock().unwrap().authenticated(),"officialConfigured":!self.key.lock().unwrap().is_empty(),"enabled":s.enabled,"playlistCount":s.albums.len(),"trackCount":s.albums.iter().map(|a|a.tracks.len()).sum::<usize>(),"updatedAt":s.updated,"job":job})
    }
    pub fn albums(&self)->Vec<Album>{let s=self.saved.lock().unwrap();if s.enabled{s.albums.clone()}else{vec![]}}
    pub fn find_track(&self,id:&str)->Option<Track>{self.saved.lock().unwrap().albums.iter().flat_map(|a|&a.tracks).find(|t|t.id==id).cloned()}
    pub fn cover_root(&self)->PathBuf{self.root.join("covers")}
    pub fn request(self:&Arc<Self>,operation:&str,body:Value)->Result<Value>{
        match operation {
            "status"=>Ok(self.status()),
            "qr"=>{if self.job.lock().unwrap()["running"]==true {bail!("正在同步曲库，请完成后重新登录")};let generation=self.generation.fetch_add(1,Ordering::SeqCst)+1;let mut web=self.web.lock().unwrap().clone();let image=web.qr()?;if self.generation.load(Ordering::SeqCst)!=generation{bail!("登录请求已取消")};*self.web.lock().unwrap()=web;Ok(json!({"image":image}))},
            "poll"=>{let generation=self.generation.load(Ordering::SeqCst);let mut web=self.web.lock().unwrap().clone();let result=web.poll()?;if self.generation.load(Ordering::SeqCst)!=generation{bail!("登录请求已取消")};*self.web.lock().unwrap()=web;Ok(result)},
            "key"=>{let key=body["key"].as_str().unwrap_or("").trim();if !key.is_empty(){OfficialClient::new(key)?.search("晴天",0)?;}*self.key.lock().unwrap()=key.into();Ok(self.status())},
            "remember"=>{let value=json!({"session":self.web.lock().unwrap().export_session(),"key":self.key.lock().unwrap().clone()});security_framework::passwords::set_generic_password("com.rhine.music.qq","connection",&serde_json::to_vec(&value)?).map_err(|_|anyhow::anyhow!("无法保存钥匙串，请检查 macOS 的授权提示"))?;std::fs::write(self.root.join("remember-connection"),b"enabled")?;self.remembered.store(true,Ordering::SeqCst);Ok(self.status())},
            "local"=>{self.import_local()?;Ok(self.status())},
            "sync"=>{self.start_sync()?;Ok(self.status())},
            "enable"=>{let mut s=self.saved.lock().unwrap();s.enabled=body["enabled"].as_bool().unwrap_or(true);atomic_json(&self.root.join("library.json"),&s.clone())?;drop(s);Ok(self.status())},
            "logout"=>{if self.remembered.load(Ordering::SeqCst){security_framework::passwords::delete_generic_password("com.rhine.music.qq","connection").map_err(|_|anyhow::anyhow!("无法移除钥匙串，请检查系统授权"))?;}let _=std::fs::remove_file(self.root.join("remember-connection"));self.remembered.store(false,Ordering::SeqCst);*self.job.lock().unwrap()=json!({"running":false});self.generation.fetch_add(1,Ordering::SeqCst);*self.web.lock().unwrap()=WebClient::new(None)?;self.key.lock().unwrap().clear();let mut s=self.saved.lock().unwrap();s.enabled=false;atomic_json(&self.root.join("library.json"),&s.clone())?;drop(s);Ok(self.status())},
            "search"|"daily"=>{
                let key=self.key.lock().unwrap().clone();let web=self.web.lock().unwrap().clone();
                let tracks=if operation=="daily"{if key.is_empty(){bail!("每日推荐需要 QQ 音乐官方 API Key，可在连接设置中填写")};OfficialClient::new(&key)?.daily()?}else if !key.is_empty(){OfficialClient::new(&key)?.search(body["query"].as_str().unwrap_or(""),0)?.0}else{vec![]};
                let mut raw=if key.is_empty(){web.search(body["query"].as_str().unwrap_or(""))?}else{tracks.iter().filter_map(|t|t.mid.as_ref().map(|mid|json!({"mid":mid,"name":t.title,"singer":t.artists.iter().map(|a|json!({"name":a})).collect::<Vec<_>>(),"type":0}))).collect()};
                // Four bounded metadata requests at a time; rendering and playback never wait on this worker.
                if !key.is_empty(){for chunk in raw.chunks_mut(4){let details=std::thread::scope(|scope|{
                    let tasks=chunk.iter().map(|t|scope.spawn(||web.song(t["mid"].as_str().unwrap_or("")).ok())).collect::<Vec<_>>();
                    tasks.into_iter().map(|h|h.join().ok().flatten()).collect::<Vec<_>>()
                });for (track,detail) in chunk.iter_mut().zip(details){if let Some(detail)=detail{*track=detail;}}}}
                let id=if operation=="daily"{"qq-daily"}else{"qq-search"};
                let cover=self.cover(id,"",raw.first());
                let mut s=self.saved.lock().unwrap();let (album,songs)=make_album(id,if operation=="daily"{"每日推荐"}else{"QQ 搜索结果"},"qq-discover",&raw,cover);
                s.songs.extend(songs);s.albums.retain(|a|a.id!=id);s.albums.push(album.clone());s.enabled=true;drop(s);
                Ok(json!({"albumId":id,"count":album.tracks.len()}))
            },
            "client"=>{Ok(json!({"url":"https://y.qq.com/"}))},
            _=>bail!("未知 QQ 音乐操作"),
        }
    }
    fn import_local(&self)->Result<()> {
        if self.job.lock().unwrap()["running"]==true {bail!("正在同步，请稍后导入本地快照")}
        let uin=self.web.lock().unwrap().uin().parse::<i64>().ok();
        let playlists=rhine_qq_connector::local::read_library(&rhine_qq_connector::local::default_path()?,uin)?;
        if playlists.is_empty(){bail!("本机 QQ 音乐没有已缓存的歌单，请先在 QQ 客户端打开歌单")}
        let mut saved=Saved{enabled:true,updated:timestamp(),..Default::default()};
        for p in playlists {let id=format!("qq-playlist-{}",if p.liked{"liked".into()}else{p.id});
            let raw=p.tracks.iter().map(|t|json!({"id":t.id,"mid":t.mid,"name":t.title,"singer":t.artists.iter().map(|a|json!({"name":a})).collect::<Vec<_>>(),"album":{"name":t.album},"type":0})).collect::<Vec<_>>();
            let (mut album,songs)=make_album(&id,&p.name,"qq-playlists",&raw,self.cover(&id,"",None));
            album.description=Some(format!("来自本机 QQ 音乐缓存，尚未验证云端是否最新。已缓存 {}/{} 个条目。",p.tracks.len(),p.advertised_count));
            saved.albums.push(album);saved.songs.extend(songs);
        }
        atomic_json(&self.root.join("library.json"),&saved)?;*self.saved.lock().unwrap()=saved;Ok(())
    }
    pub fn start_sync(self:&Arc<Self>)->Result<()> {
        let mut job=self.job.lock().unwrap();if job["running"]==true{return Ok(())}
        let web=self.web.lock().unwrap().clone();if !web.authenticated(){bail!("请先连接 QQ 音乐账户")}
        *job=json!({"running":true,"completed":0,"total":0,"message":"正在读取歌单目录"});drop(job);
        let generation=self.generation.load(Ordering::SeqCst);let this=self.clone();
        std::thread::spawn(move||{
            let outcome=this.sync_inner(&web,generation);
            if this.generation.load(Ordering::SeqCst)!=generation{return}
            *this.job.lock().unwrap()=match outcome {Ok(n)=>json!({"running":false,"completed":n,"total":n,"message":"曲库同步完成"}),Err(e)=>json!({"running":false,"error":e.to_string(),"message":"同步未完成，原曲库保留"})};
        });Ok(())
    }
    fn sync_inner(&self,web:&WebClient,generation:u64)->Result<usize>{
        let directory=web.directory()?;let created=directory["mydiss"]["list"].as_array().context("歌单目录缺失")?;
        let collected=web.collections(false)?;let albums=web.collections(true)?;
        let mut entries=vec![("liked".to_string(),"我喜欢".to_string(),true,String::new())];
        let mut seen=HashSet::new();
        for item in created.iter().chain(collected.iter()) {
            let id=value_id(&item["dissid"]);if id.is_empty()||!seen.insert(id.clone()){continue}
            let name=item["title"].as_str().or(item["dissname"].as_str()).unwrap_or("QQ 歌单");
            let cover=item["picurl"].as_str().or(item["logo"].as_str()).unwrap_or("");entries.push((id,name.into(),false,cover.into()));
        }
        let total=entries.len()+albums.len();let mut next=Saved{enabled:true,updated:timestamp(),..Default::default()};
        for (i,(id,name,liked,cover)) in entries.iter().enumerate(){
            if self.generation.load(Ordering::SeqCst)!=generation {bail!("同步已取消")}
            *self.job.lock().unwrap()=json!({"running":true,"completed":i,"total":total,"message":format!("正在同步 {name}")});
            let result=web.playlist(if *liked{0}else{id.parse()?},*liked)?;let tracks=result["tracks"].as_array().context("歌曲列表缺失")?;
            let a_id=format!("qq-playlist-{id}");let c=self.cover(&a_id,if cover.is_empty(){result["cover"].as_str().unwrap_or("")}else{cover},tracks.first());
            let (album,songs)=make_album(&a_id,name,"qq-playlists",tracks,c);next.albums.push(album);next.songs.extend(songs);
            std::thread::sleep(Duration::from_millis(180));
        }
        for (i,a) in albums.iter().enumerate(){
            if self.generation.load(Ordering::SeqCst)!=generation {bail!("同步已取消")}
            let mid=a["albummid"].as_str().context("专辑编号缺失")?;let name=a["albumname"].as_str().unwrap_or("QQ 专辑");
            *self.job.lock().unwrap()=json!({"running":true,"completed":entries.len()+i,"total":total,"message":format!("正在同步专辑 {name}")});
            let tracks=web.album(mid)?;let id=format!("qq-album-{mid}");let c=self.cover(&id,a["pic"].as_str().unwrap_or(""),tracks.first());
            let (album,songs)=make_album(&id,name,"qq-albums",&tracks,c);next.albums.push(album);next.songs.extend(songs);
        }
        if self.generation.load(Ordering::SeqCst)!=generation{bail!("同步已取消")}
        atomic_json(&self.root.join("library.json"),&next)?;*self.saved.lock().unwrap()=next;Ok(total)
    }
    fn cover(&self,id:&str,url:&str,first:Option<&Value>)->Option<Cover>{
        let cover_id=if id=="qq-search"||id=="qq-daily"{format!("{id}-{:x}",Sha256::digest(first.map(Value::to_string).unwrap_or_default().as_bytes()))}else{id.to_owned()};
        let path=self.root.join("covers").join(format!("{cover_id}.jpg"));
        if !path.is_file(){
            let fallback=first.and_then(|t|t["album"]["mid"].as_str()).filter(|m|!m.is_empty()&&m.bytes().all(|b|b.is_ascii_alphanumeric())).map(|m|format!("https://y.gtimg.cn/music/photo_new/T002R300x300M000{m}.jpg"));
            let address=if url.is_empty(){fallback.as_deref()?}else{url}.replacen("http://","https://",1);
            let url=reqwest::Url::parse(&address).ok()?;
            if url.scheme()!="https"||!url.host_str().is_some_and(|h|h.ends_with(".qq.com")||h.ends_with(".qpic.cn")||h=="y.gtimg.cn"){return None}
            let client=reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).redirect(reqwest::redirect::Policy::none()).build().ok()?;
            let response=client.get(url).send().ok()?;if !response.status().is_success(){return None}
            let mut bytes=Vec::new();response.take(4*1024*1024+1).read_to_end(&mut bytes).ok()?;if bytes.len()>4*1024*1024||bytes.len()<100{return None}
            std::fs::write(&path,bytes).ok()?;
        }
        Some(Cover{path,mime:"image/jpeg".into(),version:"qq-1".into(),embedded:true})
    }
    pub fn prepare(&self,track:&Track,token:&AtomicU64,expected:u64)->Result<Track>{
        let song=self.saved.lock().unwrap().songs.get(&track.id).cloned().context("QQ 曲目已不在曲库中")?;
        if song.mid.is_empty(){bail!("此曲目没有在线歌曲编号；请从本地文件导入，或在 QQ 客户端播放")}
        let filename=format!("{:x}.m4a",Sha256::digest(format!("{}:{}",song.mid,song.kind).as_bytes()));
        let path=self.root.join("audio").join(&filename);
        let mut ready=track.clone();ready.path=path.clone();ready.format="AAC".into();ready.codec=Some("AAC".into());
        let web=self.web.lock().unwrap().clone();
        if path.is_file()&&path.metadata()?.len()>1000{return Ok(ready)}
        let url=web.audio_url(&song.mid,song.kind)?;
        if token.load(Ordering::SeqCst)!=expected{bail!("播放请求已取消")}
        let client=reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(75)).build()?;
        let response=client.get(url).header("Referer","https://y.qq.com/").send().map_err(|_|anyhow::anyhow!("音频连接失败，请重试"))?;
        if !response.status().is_success(){bail!("音频暂时不可用（HTTP {}），请重试",response.status().as_u16())}
        if !audio_host(response.url()){bail!("音频来源不受支持")}
        let temp=path.with_extension(format!("{expected}.part"));
        let result=(||->Result<()>{let mut file=std::fs::File::create(&temp)?;let mut r=response;let mut buf=[0u8;65536];let mut size=0;
            loop{if token.load(Ordering::SeqCst)!=expected{bail!("播放请求已取消")};let n=r.read(&mut buf).map_err(|_|anyhow::anyhow!("音频读取中断，请重试"))?;if n==0{break};size+=n;if size>128*1024*1024{bail!("音频超过缓存大小限制")};file.write_all(&buf[..n])?}
            if size<1000{bail!("音频内容为空")};drop(file);std::fs::rename(&temp,&path)?;Ok(())})();
        if result.is_err(){let _=std::fs::remove_file(&temp);}result?;
        self.prune_audio(&path);Ok(ready)
    }
    fn prune_audio(&self,keep:&Path){
        let mut entries=std::fs::read_dir(self.root.join("audio")).into_iter().flatten().flatten().filter_map(|e|e.metadata().ok().map(|m|(e.path(),m.len(),m.modified().ok()))).filter(|(p,_,_)|p.extension().is_some_and(|s|s=="m4a")).collect::<Vec<_>>();
        let mut total=entries.iter().map(|(_,n,_)|n).sum::<u64>();entries.sort_by_key(|(_,_,t)|*t);
        for (path,n,_) in entries{if total<=512*1024*1024{break};if path!=keep&&std::fs::remove_file(path).is_ok(){total=total.saturating_sub(n)}}
    }
}
fn value_id(v:&Value)->String{v.as_str().map(str::to_owned).or_else(||v.as_u64().map(|n|n.to_string())).unwrap_or_default()}
fn make_album(id:&str,name:&str,genre:&str,raw:&[Value],cover:Option<Cover>)->(Album,HashMap<String,Song>){
    let mut songs=HashMap::new();let mut tracks=Vec::new();
    for (i,t) in raw.iter().enumerate(){
        let mid=t["mid"].as_str().unwrap_or("");let numeric=value_id(&t["id"]);let track_id=format!("{id}:{i}:{}",if numeric.is_empty(){mid}else{&numeric});
        songs.insert(track_id.clone(),Song{mid:mid.into(),kind:t["type"].as_u64().unwrap_or(0)});
        let artist=t["singer"].as_array().into_iter().flatten().filter_map(|s|s["name"].as_str()).collect::<Vec<_>>().join(" / ");
        tracks.push(Track{id:track_id,album_id:id.into(),title:t["title"].as_str().or(t["name"].as_str()).unwrap_or("未命名曲目").into(),artist,
            track_number:Some(i as u32+1),disc_number:Some(1),duration:t["interval"].as_f64().unwrap_or(0.),format:if mid.is_empty(){"需本地文件"}else{"QQ 音乐"}.into(),browser_playable:!mid.is_empty(),
            common:crate::library::Common{album:t["album"]["title"].as_str().or(t["album"]["name"].as_str()).unwrap_or("").into(),..Default::default()},..Default::default()});
    }
    (Album{id:id.into(),title:name.into(),artist:if genre=="qq-albums"{tracks.first().map(|t|t.artist.clone()).unwrap_or_default()}else if genre=="qq-discover"{"QQ 音乐 · 搜索与推荐".into()}else{"QQ 音乐 · 我的歌单".into()},disc_count:1,genre_id:genre.into(),tracks,cover,description:Some("来自你的 QQ 音乐曲库。在线播放以当前账户及歌曲权限为准；暂不可用的条目会保留在歌单中。".into()),..Default::default()},songs)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn manifest_preserves_order_duplicates_and_unavailable_entries() {
        let raw=vec![json!({"id":1,"mid":"A","title":"One"}),json!({"id":2,"title":"Unavailable"}),json!({"id":1,"mid":"A","title":"One"})];
        let (album,songs)=make_album("qq-test","Test","qq-playlists",&raw,None);
        assert_eq!(album.tracks.len(),3);assert_eq!(songs.len(),3);
        assert_ne!(album.tracks[0].id,album.tracks[2].id);
        assert!(!album.tracks[1].browser_playable);
    }
    #[test]
    #[ignore = "requires the owner's in-memory QQ session, network and macOS audio device; muted"]
    fn live_qq_library_and_audio() {
        use crate::audio::{Audio,Playback};use rodio::Source;use std::time::Instant;
        let dir=std::env::var("RHINE_QQ_TEST_DATA").expect("explicit test directory required");
        let qq=Qq::new(Path::new(&dir)).unwrap();assert_eq!(qq.status()["connected"],true);
        qq.start_sync().unwrap();let started=Instant::now();
        while qq.status()["job"]["running"]==true {assert!(started.elapsed().as_secs()<240,"sync timeout");std::thread::sleep(Duration::from_millis(200));}
        let status=qq.status();assert!(status["job"]["error"].is_null(),"{}",status["job"]["error"]);
        let albums=qq.albums();assert!(!albums.is_empty());
        qq.request("search",json!({"query":"晴天"})).unwrap();
        let search=qq.albums().into_iter().find(|a|a.id=="qq-search").unwrap();assert!(!search.tracks.is_empty());
        let token=AtomicU64::new(1);let mut ready=Vec::new();let mut samples=Vec::new();
        for track in search.tracks.iter().take(6) {
            let began=Instant::now();if let Ok(mut track)=qq.prepare(track,&token,1){
                let mut decoder=crate::qq_audio::QqAudio::open(&track.path).unwrap();
                assert!(decoder.next().is_some());let rate=decoder.sample_rate();let channels=decoder.channels();
                let duration=decoder.total_duration().map(|d|d.as_secs_f64()).unwrap_or(track.duration);
                assert!(duration>60.,"sample is not full length");
                let decoded=1+decoder.count();assert!((decoded as f64/(rate as f64*channels as f64)-duration).abs()<1.);track.duration=duration;
                samples.push(json!({"sampleRate":rate,"channels":channels,"duration":duration,"bytes":track.path.metadata().unwrap().len(),"prepareMs":began.elapsed().as_millis()}));
                ready.push(track);if ready.len()==2{break}
            }
        }
        assert_eq!(ready.len(),2,"need two account-playable songs");
        let assets=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/public");
        let resolver=qq.clone();let player=Audio::with_resolver(assets,Some(Arc::new(move|t,e,n|resolver.prepare(t,e,n))));
        player.settings(0.,0.,0.,false,false);
        let wait=|condition:&dyn Fn(&Playback)->bool|{let start=Instant::now();loop{let s=player.snapshot();assert!(s.error.is_none(),"{:?}",s.error);if condition(&s){return s}assert!(start.elapsed().as_secs()<25,"transport timeout: {}",s.transport);std::thread::sleep(Duration::from_millis(20));}};
        player.play(ready.clone(),0);wait(&|s|s.playing&&s.elapsed>0.15);
        player.toggle();wait(&|s|s.transport=="paused");player.seek(45.);wait(&|s|s.elapsed>=44.);
        player.toggle();wait(&|s|s.playing);
        let began=Instant::now();for i in 0..20 {player.play(ready.clone(),i%2);std::thread::sleep(Duration::from_millis(15));}
        wait(&|s|s.playing&&s.track.as_ref().is_some_and(|t|t.id==ready[1].id));let rapid_ms=began.elapsed().as_millis();
        player.play(ready.clone(),0);wait(&|s|s.playing&&s.track.as_ref().is_some_and(|t|t.id==ready[0].id));
        player.seek(ready[0].duration-0.2);wait(&|s|s.playing&&s.track.as_ref().is_some_and(|t|t.id==ready[1].id));
        player.stop();wait(&|s|s.transport=="idle"&&!s.playing);
        let report=json!({"sync":status,"fullLengthAudioSamples":samples,"pauseSeekResume":true,"automaticNext":true,"rapidSwitchRequests":20,"rapidSwitchTotalMs":rapid_ms,"stop":true,"credentialsStored":false});
        atomic_json(&Path::new(&dir).join("live-qq-test.json"),&report).unwrap();
        println!("QQ live integration: full library, two full-length AAC decodes, pause/seek/resume, rapid switching, queue advance and stop passed.");
    }
}
