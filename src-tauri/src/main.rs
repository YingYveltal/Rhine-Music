#[cfg(feature = "preview")]
mod preview;
mod audio;
mod apple;
mod player;
mod library;
mod online;
mod qq;
mod qq_session;
mod qq_audio;
use anyhow::{bail, Context, Result};
use library::{timestamp, Library, Rules};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{Manager, State};

#[derive(Clone, Default)]
struct Jobs {
    scan: Value,
    enrich: Value,
    introductions: Value,
}
struct Core {
    library: Mutex<Library>,
    jobs: Mutex<Jobs>,
    audio: player::Player,
    apple: Arc<apple::Apple>,
    qq:Arc<qq::Qq>,
}
fn snapshot(core: &Core, app: &tauri::AppHandle) -> Result<Value> {
    let jobs = core.jobs.lock().unwrap().clone();
    let mut lib = core.library.lock().unwrap();
    let file = lib.dir.join("genre-rules.json");
    if let Ok(bytes) = std::fs::read(file) {
        if let Ok(rules) = serde_json::from_slice::<Rules>(&bytes) {
            if rules.validate().is_ok() {
                lib.rules = rules;
                lib.classify();
            }
        }
    }
    let mut albums = serde_json::to_value(&lib.index.albums)?;
    for (album, a) in albums
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(lib.index.albums.iter())
    {
        if let Some(c) = &a.cover {
            if let Ok(path) = c.path.canonicalize() {
                let allowed = if c.embedded {
                    lib.dir
                        .join("artwork")
                        .canonicalize()
                        .is_ok_and(|root| path.starts_with(root))
                } else {
                    a.root
                        .canonicalize()
                        .is_ok_and(|root| path.starts_with(root))
                };
                if allowed {
                    app.asset_protocol_scope().allow_file(&path)?;
                    album["nativeCoverPath"] = json!(path);
                }
            }
        }
        if let Some(tracks) = album["tracks"].as_array_mut() {
            for track in tracks {
                if let Some(o) = track.as_object_mut() {
                    o.remove("_path");
                    o.remove("_common");
                    o.remove("_embeddedCover");
                }
            }
        }
    }
    let mut genres=lib.genres();
    for album in core.qq.albums(){
        if !genres.iter().any(|g|g.id==album.genre_id){genres.push(library::Genre{id:album.genre_id.clone(),name:match album.genre_id.as_str(){"qq-albums"=>"QQ 收藏专辑","qq-discover"=>"QQ 发现",_=>"QQ 歌单"}.into(),aliases:vec![]})}
        let mut value=serde_json::to_value(&album)?;
        if let Some(c)=&album.cover {if let Ok(path)=c.path.canonicalize(){if path.starts_with(core.qq.cover_root().canonicalize()?){app.asset_protocol_scope().allow_file(&path)?;value["nativeCoverPath"]=json!(path)}}}
        if let Some(ts)=value["tracks"].as_array_mut(){for t in ts{if let Some(o)=t.as_object_mut(){for k in ["_path","_common","_embeddedCover","_fingerprint"]{o.remove(k);}}}}
        albums.as_array_mut().unwrap().push(value);
    }
    for mut album in core.apple.albums() {
        if let Some(path)=album["nativeCoverPath"].as_str().map(PathBuf::from) {
            if let Ok(path)=path.canonicalize() {
                if path.starts_with(core.apple.cover_root().canonicalize()?) { app.asset_protocol_scope().allow_file(&path)?; }
                else { album.as_object_mut().unwrap().remove("nativeCoverPath"); }
            }
        }
        albums.as_array_mut().unwrap().push(album);
    }
    genres.push(library::Genre{id:"apple-playlists".into(),name:"Apple Music 歌单".into(),aliases:vec![]});
    Ok(
        json!({"version":1,"albums":albums,"genres":genres,"qq":core.qq.status(),"apple":core.apple.status(),"roots":lib.config.roots.iter().map(|path|lib.index.roots.iter().find(|r|&r.path==path).map(|r|serde_json::to_value(r).unwrap()).unwrap_or(json!({"path":path,"status":"unscanned"}))).collect::<Vec<_>>(),"scan":if jobs.scan.is_null(){json!({"running":false})}else{jobs.scan.clone()},"enrich":if jobs.enrich.is_null(){json!({"running":false,"completed":0,"total":0})}else{jobs.enrich.clone()},"introductions":if jobs.introductions.is_null(){json!({"running":false,"completed":0,"total":0,"updated":0,"notFound":0,"failed":0})}else{jobs.introductions.clone()},"onlineEnabled":lib.config.online_enabled}),
    )
}
fn any_job(j: &Jobs) -> bool {
    j.scan["running"] == true || j.enrich["running"] == true || j.introductions["running"] == true
}
fn start_scan(core: Arc<Core>, app: tauri::AppHandle, body: &Value) -> Result<()> {
    let mut jobs = core.jobs.lock().unwrap();
    if jobs.scan["running"] == true {
        return Ok(());
    }
    if any_job(&jobs) {
        bail!("正在查询资料，请完成后重新扫描")
    }
    let mut lib = core.library.lock().unwrap().clone();
    if let Some(roots) = body.get("roots") {
        lib.config.roots = library::safe_roots(serde_json::from_value(roots.clone())?)?;
        lib.save()?;
        *core.library.lock().unwrap() = lib.clone();
    }
    jobs.scan = json!({"running":true,"startedAt":timestamp()});
    drop(jobs);
    std::thread::spawn(move || {
        let result = lib.scan(|progress| {
            core.jobs.lock().unwrap().scan["message"] = json!(progress);
        });
        let mut automatic = false;
        match result {
            Ok(()) => {
                {
                    let mut current = core.library.lock().unwrap();
                    lib.config = current.config.clone();
                    lib.rules = current.rules.clone();
                    lib.classify();
                    if let Err(e) = lib.save() {
                        drop(current);
                        core.jobs.lock().unwrap().scan = json!({"running":false,"finishedAt":timestamp(),"error":format!("保存扫描结果失败：{e}")});
                        return;
                    }
                    automatic =
                        lib.config.online_enabled && !lib.config.music_brainz_contact.is_empty();
                    *current = lib;
                }
                core.jobs.lock().unwrap().scan = json!({"running":false,"finishedAt":timestamp()});
            }
            Err(e) => {
                core.jobs.lock().unwrap().scan =
                    json!({"running":false,"finishedAt":timestamp(),"error":e.to_string()})
            }
        };
        if automatic {
            let _ = start_online(core, app, &json!({}), false);
        }
    });
    Ok(())
}
fn start_online(
    core: Arc<Core>,
    _app: tauri::AppHandle,
    body: &Value,
    introductions: bool,
) -> Result<()> {
    let mut jobs = core.jobs.lock().unwrap();
    if any_job(&jobs) {
        bail!("已有后台任务正在运行，请稍后再试")
    }
    let mut lib = core.library.lock().unwrap().clone();
    if !introductions && lib.config.music_brainz_contact.is_empty() {
        bail!("请先填写 MusicBrainz 联系邮箱或项目网址")
    }
    let ids: Option<Vec<String>> = body
        .get("albumIds")
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()?;
    let force = body["force"].as_bool().unwrap_or(false);
    let indices = lib
        .index
        .albums
        .iter()
        .enumerate()
        .filter(|(_, a)| ids.as_ref().is_none_or(|ids| ids.contains(&a.id)))
        .filter(|(_, a)| {
            force
                || if introductions {
                    let cached = &a.introduction;
                    cached["status"] == "error"
                        || cached["checkedAt"]
                            .as_str()
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .is_none_or(|t| {
                                chrono::Utc::now().signed_duration_since(t).num_days() >= 7
                            })
                } else {
                    a.online["status"].as_str().unwrap_or("unqueried") == "unqueried"
                }
        })
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let total = indices.len();
    let status =
        json!({"running":true,"completed":0,"total":total,"updated":0,"notFound":0,"failed":0});
    if introductions {
        jobs.introductions = status
    } else {
        jobs.enrich = status
    }
    drop(jobs);
    std::thread::spawn(move || {
        let mut completed = 0;
        let mut updated = 0;
        let mut missing = 0;
        let mut failed = 0;
        let mut consecutive = 0;
        let mut error = None;
        match online::Online::new(&lib.config.music_brainz_contact) {
            Err(e) => error = Some(e.to_string()),
            Ok(mut online) => {
                for i in indices {
                    {
                        let mut jobs = core.jobs.lock().unwrap();
                        let job = if introductions {
                            &mut jobs.introductions
                        } else {
                            &mut jobs.enrich
                        };
                        job["currentAlbum"] = json!(lib.index.albums[i].title);
                    }
                    let result = if introductions {
                        online.introduction(&mut lib.index.albums[i])
                    } else {
                        online.enrich(&mut lib.index.albums[i])
                    };
                    completed += 1;
                    if let Err(e) = result {
                        failed += 1;
                        consecutive += 1;
                        if consecutive >= 3 {
                            error = Some(format!("资料来源连续失败，已暂停：{e}"));
                        }
                    } else {
                        consecutive = 0;
                        if (introductions
                            && lib.index.albums[i].introduction["status"] == "matched")
                            || (!introductions && lib.index.albums[i].online["status"] == "matched")
                        {
                            updated += 1
                        } else {
                            missing += 1
                        }
                    }
                    {
                        let mut current = core.library.lock().unwrap();
                        lib.config = current.config.clone();
                        lib.rules = current.rules.clone();
                        lib.classify();
                        if let Err(e) = lib.save() {
                            error = Some(e.to_string());
                        }
                        *current = lib.clone();
                    }
                    {
                        let mut jobs = core.jobs.lock().unwrap();
                        let job = if introductions {
                            &mut jobs.introductions
                        } else {
                            &mut jobs.enrich
                        };
                        *job = json!({"running":true,"completed":completed,"total":total,"updated":updated,"notFound":missing,"failed":failed});
                    }
                    if error.is_some() {
                        break;
                    }
                }
            }
        }
        let mut jobs = core.jobs.lock().unwrap();
        let job = if introductions {
            &mut jobs.introductions
        } else {
            &mut jobs.enrich
        };
        *job = json!({"running":false,"completed":completed,"total":total,"updated":updated,"notFound":missing,"failed":failed,"error":error});
    });
    Ok(())
}
#[tauri::command]
async fn music_request(
    app: tauri::AppHandle,
    core: State<'_, Arc<Core>>,
    route: String,
    body: Option<Value>,
) -> std::result::Result<Value, String> {
    request_inner(&app, core.inner().clone(), &route, body).map_err(|e| e.to_string())
}
fn request_inner(
    app: &tauri::AppHandle,
    core: Arc<Core>,
    route: &str,
    body: Option<Value>,
) -> Result<Value> {
    match route {
        "/api/library" => snapshot(&core, app),
        "/api/library/scan" => {
            start_scan(core.clone(), app.clone(), &body.unwrap_or(json!({})))?;
            snapshot(&core, app)
        }
        "/api/library/enrich" | "/api/library/introductions" => {
            start_online(
                core.clone(),
                app.clone(),
                &body.unwrap_or(json!({})),
                route.ends_with("introductions"),
            )?;
            snapshot(&core, app)
        }
        "/api/config" | "/api/library/config" => {
            if let Some(body) = body {
                if any_job(&core.jobs.lock().unwrap()) && body.get("roots").is_some() {
                    bail!("后台任务运行期间不能修改目录")
                }
                let mut lib = core.library.lock().unwrap();
                let mut config = lib.config.clone();
                if let Some(roots) = body.get("roots") {
                    config.roots = library::safe_roots(serde_json::from_value(roots.clone())?)?;
                }
                if let Some(enabled) = body.get("onlineEnabled") {
                    config.online_enabled =
                        enabled.as_bool().context("onlineEnabled 必须是布尔值")?;
                }
                if let Some(contact) = body.get("musicBrainzContact") {
                    let contact = contact.as_str().context("联系方式必须是文本")?.trim();
                    if contact.len() > 500
                        || contact.contains(['\r', '\n'])
                        || (!contact.is_empty()
                            && !regex::Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$|^https?://\S+$")
                                .unwrap()
                                .is_match(contact))
                    {
                        bail!("请输入有效的联系邮箱或项目网址")
                    };
                    config.music_brainz_contact = contact.into();
                }
                lib.config = config;
                lib.save()?;
            }
            let lib = core.library.lock().unwrap();
            let mut config = serde_json::to_value(&lib.config)?;
            config["musicBrainzConfigured"] = json!(!lib.config.music_brainz_contact.is_empty());
            Ok(config)
        }
        "/api/genre-rules" => {
            let mut lib = core.library.lock().unwrap();
            if let Some(body) = body {
                let rules: Rules = serde_json::from_value(body)?;
                rules.validate()?;
                let file = lib.dir.join("genre-rules.json");
                std::fs::copy(&file, file.with_extension("json.backup"))?;
                lib.rules = rules;
                lib.classify();
                lib.save()?;
            }
            Ok(serde_json::to_value(&lib.rules)?)
        }
        _ => bail!("不支持的本地操作：{route}"),
    }
}
#[tauri::command]
fn toggle_fullscreen(window: tauri::WebviewWindow) -> std::result::Result<(), String> {
    window
        .is_fullscreen()
        .and_then(|current| window.set_fullscreen(!current))
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn qq_request(core:State<'_,Arc<Core>>,operation:String,body:Option<Value>)->std::result::Result<Value,String>{
    if operation=="logout" {let _ = core.audio.send(player::Command::Stop);}
    let qq=core.qq.clone();
    tauri::async_runtime::spawn_blocking(move||qq.request(&operation,body.unwrap_or(json!({}))).map_err(|e|e.to_string())).await.map_err(|_|"QQ 后台任务未完成".to_owned())?
}
#[tauri::command]
async fn apple_request(core:State<'_,Arc<Core>>,operation:String,body:Option<Value>)->std::result::Result<Value,String>{
    let body=body.unwrap_or(json!({}));
    if operation=="enable" && body["enabled"]==false {
        core.audio.send(player::Command::DisableApple).map_err(|e|e.to_string())?;
    }
    let apple=core.apple.clone();
    tauri::async_runtime::spawn_blocking(move||apple.request(&operation,body).map_err(|e|e.to_string()))
        .await.map_err(|_|"Apple Music 后台任务未完成".to_owned())?
}
#[tauri::command]
fn player_state(core: State<'_, Arc<Core>>) -> Value {
    core.audio.snapshot()
}
#[tauri::command]
fn player_command(
    core: State<'_, Arc<Core>>,
    operation: String,
    id: Option<String>,
    ids: Option<Vec<String>>,
    value: Option<Value>,
) -> std::result::Result<u64, String> {
    (|| -> Result<u64> {
        match operation.as_str() {
            "play" => {
                let id = id.context("缺少曲目 ID")?;
                let ids = ids.unwrap_or_else(|| vec![id.clone()]);
                if id.starts_with("apple-track-") {
                    core.apple.playable()?;
                    anyhow::ensure!(ids.iter().all(|id|id.starts_with("apple-track-")),"首版不支持跨来源混合队列");
                    return core.audio.send(player::Command::ApplePlay{id,ids});
                }
                let lib = core.library.lock().unwrap();
                let mut tracks = Vec::new();
                for id in ids {
                    if id.starts_with("qq-"){tracks.push(core.qq.find_track(&id).context("QQ 曲目已不在曲库中")?);continue}
                    let (album, track) = lib
                        .index
                        .albums
                        .iter()
                        .flat_map(|a| a.tracks.iter().map(move |t| (a, t)))
                        .find(|(_, t)| t.id == id)
                        .context("曲目已不在音乐库中")?;
                    let actual = track.path.canonicalize()?;
                    let root = album.root.canonicalize()?;
                    if !actual.starts_with(&root) || !lib.config.roots.contains(&album.root) {
                        bail!("歌曲已移出配置的音乐目录")
                    };
                    tracks.push(track.clone());
                }
                let index = tracks
                    .iter()
                    .position(|t| t.id == id)
                    .context("歌曲不在队列中")?;
                core.audio.send(player::Command::Play(tracks, index))
            }
            "toggle" => core.audio.send(player::Command::Toggle),
            "stop" => core.audio.send(player::Command::Stop),
            "unlock" => core.audio.send(player::Command::Unlock),
            "next" => core.audio.send(player::Command::Next),
            "previous" => core.audio.send(player::Command::Previous),
            "seek" => {
                let seconds=value.and_then(|v| v.as_f64()).context("位置必须为数字")?;
                anyhow::ensure!(seconds.is_finite() && seconds>=0.,"位置必须为非负有限数字");
                core.audio.send(player::Command::Seek(seconds))
            },
            "settings" => {
                let v = value.context("缺少设置")?;
                let volume = |key: &str, default: f64| {
                    v[key].as_f64().unwrap_or(default).clamp(0., 1.) as f32
                };
                core.audio.send(player::Command::Settings(
                    volume("volume", 0.65),
                    volume("bgmVolume", 0.18),
                    0.,
                    v["bgmEnabled"].as_bool().unwrap_or(true),
                    v["songFadeEnabled"].as_bool().unwrap_or(true),
                ))
            }
            _ => bail!("不支持的播放操作"),
        }
    })()
    .map_err(|e| e.to_string())
}
#[tauri::command]
fn save_render_capture(
    core: State<'_, Arc<Core>>,
    id: String,
    name: String,
    data: String,
) -> std::result::Result<(), String> {
    use base64::Engine;
    if id.is_empty()
        || id.len() > 100
        || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        || ![
            "scene.json",
            "color.rgba16f",
            "depth.rgba16f",
            "bokeh.rgba16f",
            "reference.rgba8",
            "gl-frame.json",
        ]
        .contains(&name.as_str()) && !regex::Regex::new(r"^data-[0-9]{3}\.bin$").unwrap().is_match(&name)
        || data.len() > 100_000_000
    {
        return Err("Invalid render capture".into());
    }
    let dir = core
        .library
        .lock()
        .unwrap()
        .dir
        .join("metal-captures")
        .join(id);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())
}
#[tauri::command]
fn save_benchmark(core: State<'_, Arc<Core>>, report: Value) -> std::result::Result<(), String> {
    let lib = core.library.lock().unwrap();
    let label = report["label"]
        .as_str()
        .unwrap_or("measurement")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(80)
        .collect::<String>();
    let path = lib.dir.join("benchmarks").join(format!(
        "{}-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S"),
        label
    ));
    library::atomic_json(&path, &report).map_err(|e| e.to_string())
}
// Return selected paths only; saving and scanning retain their existing validation.
#[tauri::command]
async fn pick_music_folders(window: tauri::WebviewWindow) -> std::result::Result<Option<Vec<String>>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let selected = window.dialog().file().set_parent(&window)
            .set_title("选择音乐文件夹").set_can_create_directories(false).blocking_pick_folders();
        selected.map(|paths| paths.into_iter().map(|file| {
            let path = file.into_path().map_err(|e| e.to_string())?;
            let text = path.to_str().ok_or_else(|| "文件夹路径无法显示为文本".to_string())?;
            if text.contains(['\n', '\r']) || text.trim() != text {
                return Err("目录输入暂不支持带换行或首尾空白的路径，请先重命名文件夹".to_string());
            }
            Ok(text.to_owned())
        }).collect::<std::result::Result<Vec<_>, String>>()).transpose()
    }).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn open_link(app: tauri::AppHandle, href: String) -> std::result::Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let url = reqwest::Url::parse(&href).map_err(|e| e.to_string())?;
    if matches!(url.scheme(), "http" | "https") {
        return app
            .opener()
            .open_url(href, None::<&str>)
            .map_err(|e| e.to_string());
    }
    if url.scheme() != "tauri" || url.host_str() != Some("localhost") {
        return Err("不支持的链接".into());
    }
    let name = match url.path() {
        "/licenses/project-mit.txt" => "license",
        "/fonts/MiSans-license.pdf" => "font-license",
        "/" if url.query() == Some("original=1&scene=archive") => "archive",
        _ => return Err("未找到本地页面".into()),
    };
    let label = format!("reference-{name}");
    if let Some(window) = app.get_webview_window(&label) {
        return window.set_focus().map_err(|e| e.to_string());
    }
    let builder = tauri::WebviewWindowBuilder::new(&app, label, tauri::WebviewUrl::External(url));
    #[cfg(feature = "preview")]
    let builder = builder.data_store_identifier(preview::identity().1);
    builder.title("Rhine Music · 参考资料")
        .inner_size(1100.0, 760.0)
        .build()
        .map(|_| ())
        .map_err(|e| e.to_string())
}
fn assets(app: &tauri::App) -> PathBuf {
    let bundled = app.path().resource_dir().unwrap();
    if bundled.join("audio").is_dir() {
        bundled
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frontend/public")
    }
}
fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[cfg(feature = "preview")]
            let data = preview::setup(app)?;
            #[cfg(not(feature = "preview"))]
            let data = std::env::var_os("MUSIC_NATIVE_DATA_DIR")
                .map(PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            let lib = Library::open(data)?;
            let qq=qq::Qq::new(&lib.dir)?;
            let apple=apple::Apple::new(&lib.dir)?;
            let resolver_qq=qq.clone();
            let scan = !lib.config.roots.is_empty();
            let core = Arc::new(Core {
                library: Mutex::new(lib),
                jobs: Mutex::new(Jobs::default()),
                audio: player::Player::new(Arc::new(audio::Audio::with_resolver(assets(app),Some(Arc::new(move|track,token,expected|resolver_qq.prepare(track,token,expected))))),apple.native.clone()),
                apple,
                qq:qq.clone(),
            });
            app.manage(core.clone());
            if qq.status()["connectionState"]=="unverified" {
                let connection=qq.clone();
                std::thread::spawn(move||{
                    if connection.request("validate",json!({})).is_ok() && connection.albums().is_empty(){let _=connection.start_sync();}
                });
            }
            if scan {
                start_scan(core, app.handle().clone(), &json!({}))?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            music_request,
            player_command,
            player_state,
            qq_request,
            apple_request,
            save_benchmark,
            save_render_capture,
            open_link,
            pick_music_folders,
            toggle_fullscreen
        ])
        .run(tauri::generate_context!())
        .expect("Rhine Music failed to launch");
}
