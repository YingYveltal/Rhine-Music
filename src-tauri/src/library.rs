use anyhow::{bail, Context, Result};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    prelude::Accessor,
    tag::ItemKey,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub album_id: String,
    pub title: String,
    pub artist: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub duration: f64,
    pub format: String,
    pub codec: Option<String>,
    pub bits_per_sample: Option<u8>,
    pub sample_rate: Option<u32>,
    pub bitrate: Option<u32>,
    pub number_of_channels: Option<u8>,
    pub lossless: Option<bool>,
    pub browser_playable: bool,
    pub audio_url: String,
    pub relative_path: String,
    #[serde(rename = "_path")]
    pub path: PathBuf,
    #[serde(rename = "_fingerprint")]
    pub fingerprint: String,
    #[serde(rename = "_common")]
    pub common: Common,
    #[serde(rename = "_embeddedCover")]
    pub embedded_cover: Option<Cover>,
    pub metadata_error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Common {
    pub album: String,
    pub albumartist: String,
    pub year: Option<u32>,
    pub genres: Vec<String>,
    pub producers: Vec<String>,
    pub comments: Vec<String>,
    pub release_id: String,
    pub release_group_id: String,
    pub disc_total: Option<u32>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Cover {
    pub path: PathBuf,
    pub mime: String,
    pub version: String,
    pub embedded: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
    pub disc_count: u32,
    pub description: Option<String>,
    pub description_source: Option<Value>,
    pub local_note: Option<String>,
    pub introduction: Value,
    pub genre_id: String,
    pub raw_genres: Vec<String>,
    pub folder: PathBuf,
    pub tracks: Vec<Track>,
    pub producers: Vec<Value>,
    pub offline: bool,
    pub online: Value,
    #[serde(rename = "_root")]
    pub root: PathBuf,
    #[serde(rename = "_cover")]
    pub cover: Option<Cover>,
    #[serde(rename = "_localGenres")]
    pub local_genres: Vec<String>,
    #[serde(rename = "_onlineGenres")]
    pub online_genres: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Genre {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rules {
    pub version: u32,
    pub genres: Vec<Genre>,
    pub album_overrides: HashMap<String, String>,
}
impl Default for Rules {
    fn default() -> Self {
        let rows = [
            (
                "mandopop",
                "华语流行",
                "Mandopop|国语流行音乐|华语流行音乐|华语流行|国语流行|Chinese Pop",
            ),
            ("pop", "流行", "Pop|流行音乐"),
            ("rock", "摇滚", "Rock|摇滚音乐"),
            ("jazz", "爵士", "Jazz|爵士乐"),
            ("classical", "古典", "Classical|古典音乐"),
            ("electronic", "电子", "Electronic|Electronica|电子音乐"),
            ("ambient", "氛围", "Ambient|氛围音乐"),
            ("soundtrack", "原声", "Soundtrack|OST|原声音乐|电影原声"),
            ("unclassified", "未分类", ""),
        ];
        Self {
            version: 1,
            genres: rows
                .into_iter()
                .map(|(id, name, a)| Genre {
                    id: id.into(),
                    name: name.into(),
                    aliases: a
                        .split('|')
                        .filter(|x| !x.is_empty())
                        .map(str::to_owned)
                        .collect(),
                })
                .collect(),
            album_overrides: HashMap::new(),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub version: u32,
    pub roots: Vec<PathBuf>,
    pub online_enabled: bool,
    pub music_brainz_contact: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Root {
    pub path: PathBuf,
    pub status: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Index {
    pub version: u32,
    pub albums: Vec<Album>,
    pub roots: Vec<Root>,
    pub scanned_at: Option<String>,
}
#[derive(Clone)]
pub struct Library {
    pub dir: PathBuf,
    pub config: Config,
    pub rules: Rules,
    pub index: Index,
}

pub fn hash(v: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(v.as_ref()))[..24].to_owned()
}
pub fn normalized(v: &str) -> String {
    v.nfkc()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}
pub fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    fs::create_dir_all(path.parent().context("数据路径无父目录")?)?;
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(&temp, path)?;
    Ok(())
}
fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match fs::read(path) {
        Ok(b) => Ok(serde_json::from_slice(&b)
            .with_context(|| format!("无法读取 {}，已保留原文件", path.display()))?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}
pub fn safe_roots(roots: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    if roots.len() > 64 {
        bail!("音乐目录最多 64 个")
    }
    let mut clean = Vec::new();
    for path in roots {
        if !path.is_absolute() || path.to_string_lossy().contains('\0') {
            bail!("音乐目录必须为绝对路径")
        }
        let path = if path.exists() {
            path.canonicalize()?
        } else {
            let mut normalized = PathBuf::new();
            for part in path.components() {
                match part {
                    std::path::Component::ParentDir => {
                        normalized.pop();
                    }
                    std::path::Component::CurDir => {}
                    other => normalized.push(other.as_os_str()),
                }
            }
            normalized
        };
        if !clean.contains(&path) {
            clean.push(path);
        }
    }
    Ok(clean
        .iter()
        .filter(|p| !clean.iter().any(|q| p != &q && p.starts_with(q)))
        .cloned()
        .collect())
}
impl Rules {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 || self.genres.len() > 1000 {
            bail!("流派规则版本或数量不正确")
        }
        let mut ids = std::collections::HashSet::new();
        for g in &self.genres {
            if g.id.is_empty()
                || !g
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                || g.name.trim().is_empty()
                || !ids.insert(&g.id)
            {
                bail!("流派 ID 必须唯一，名称不能为空")
            }
        }
        for id in self.album_overrides.values() {
            if !ids.contains(id) {
                bail!("人工分类引用了不存在的流派：{id}")
            }
        }
        Ok(())
    }
    pub fn resolve(&self, a: &Album) -> String {
        if let Some(id) = self.album_overrides.get(&a.id) {
            return id.clone();
        }
        let inputs: Vec<_> = a
            .online_genres
            .iter()
            .chain(a.local_genres.iter())
            .chain(a.raw_genres.iter())
            .collect();
        for input in &inputs {
            let n = normalized(input);
            for g in &self.genres {
                if std::iter::once(&g.id)
                    .chain(std::iter::once(&g.name))
                    .chain(g.aliases.iter())
                    .any(|x| normalized(x) == n)
                {
                    return g.id.clone();
                }
            }
        }
        inputs
            .first()
            .map(|s| format!("source-{}", hash(normalized(s))))
            .unwrap_or_else(|| "unclassified".into())
    }
}
impl Library {
    pub fn open(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut config: Config = read_json(&dir.join("config.json"))?;
        config.version = 1;
        config.roots = safe_roots(config.roots)?;
        let mut index: Index = read_json(&dir.join("library-index.json"))?;
        if index.version > 1 {
            bail!("曲库索引版本不受支持，已保留原文件")
        };
        index.version = 1;
        let rules: Rules = read_json(&dir.join("genre-rules.json"))?;
        rules.validate()?;
        let mut lib = Self {
            dir,
            config,
            rules,
            index,
        };
        lib.classify();
        lib.save()?;
        Ok(lib)
    }
    pub fn save(&self) -> Result<()> {
        atomic_json(&self.dir.join("config.json"), &self.config)?;
        atomic_json(&self.dir.join("genre-rules.json"), &self.rules)?;
        atomic_json(&self.dir.join("library-index.json"), &self.index)
    }
    pub fn classify(&mut self) {
        for a in &mut self.index.albums {
            a.genre_id = self.rules.resolve(a);
        }
    }
    pub fn genres(&self) -> Vec<Genre> {
        let mut genres = self.rules.genres.clone();
        for a in &self.index.albums {
            if !genres.iter().any(|g| g.id == a.genre_id) {
                genres.push(Genre {
                    id: a.genre_id.clone(),
                    name: a
                        .online_genres
                        .first()
                        .or(a.local_genres.first())
                        .or(a.raw_genres.first())
                        .cloned()
                        .unwrap_or_else(|| "未分类".into()),
                    aliases: vec![],
                });
            }
        }
        genres.retain(|g| self.index.albums.iter().any(|a| a.genre_id == g.id));
        genres
    }
    pub fn scan(&mut self, mut progress: impl FnMut(String)) -> Result<()> {
        let mut albums = Vec::new();
        let mut roots = Vec::new();
        for root in &self.config.roots {
            let previous: Vec<_> = self
                .index
                .albums
                .iter()
                .filter(|a| &a.root == root)
                .collect();
            let result = (|| -> Result<Vec<Album>> {
                if !root.is_dir() {
                    bail!("目录不存在或暂时离线")
                }
                let entries = walk_albums(root)?;
                let mut next = Vec::new();
                for (n, entry) in entries.iter().enumerate() {
                    progress(format!(
                        "正在扫描 {}/{} · {}",
                        n + 1,
                        entries.len(),
                        entry
                            .folder
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ));
                    let id = entry.id();
                    let old = previous.iter().find(|a| a.id == id).copied();
                    next.push(self.read_album(root, entry, old)?);
                }
                Ok(next)
            })();
            match result {
                Ok(next) => {
                    albums.extend(next);
                    roots.push(Root {
                        path: root.clone(),
                        status: "online".into(),
                        error: None,
                    });
                }
                Err(e) => {
                    for a in previous {
                        let mut a = a.clone();
                        a.offline = true;
                        albums.push(a);
                    }
                    roots.push(Root {
                        path: root.clone(),
                        status: "offline".into(),
                        error: Some(e.to_string()),
                    });
                }
            }
        }
        self.index = Index {
            version: 1,
            albums,
            roots,
            scanned_at: Some(timestamp()),
        };
        self.classify();
        Ok(())
    }
    fn read_album(&self, root: &Path, entry: &Entry, old: Option<&Album>) -> Result<Album> {
        let id = entry.id();
        let mut tracks = Vec::new();
        let mut cover = if entry.single {
            None
        } else {
            entry.cover.as_ref().map(file_cover).transpose()?
        };
        for file in &entry.tracks {
            let meta = fs::metadata(file)?;
            let fp = format!(
                "{}:{}",
                meta.len(),
                meta.modified()?.duration_since(UNIX_EPOCH)?.as_nanos()
            );
            let mut track = if let Some(cached) = old.and_then(|a| {
                a.tracks
                    .iter()
                    .find(|t| t.path == *file && t.fingerprint == fp)
            }) {
                cached.clone()
            } else {
                read_track(root, file, &id, &fp, &self.dir)?
            };
            if cover.is_none() {
                cover = track.embedded_cover.clone().filter(|c| c.path.exists());
            }
            track.album_id = id.clone();
            tracks.push(track);
        }
        if cover.is_none() {
            cover = entry.cover.as_ref().map(file_cover).transpose()?;
        }
        tracks.sort_by(|a, b| {
            a.disc_number
                .unwrap_or(1)
                .cmp(&b.disc_number.unwrap_or(1))
                .then(
                    a.track_number
                        .unwrap_or(9999)
                        .cmp(&b.track_number.unwrap_or(9999)),
                )
                .then(natural_key(&a.relative_path).cmp(&natural_key(&b.relative_path)))
        });
        let first = tracks.first().context("专辑内没有曲目")?;
        let title = if entry.single {
            first.title.clone()
        } else if !first.common.album.is_empty() {
            first.common.album.clone()
        } else {
            entry
                .folder
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        };
        let artist = if !entry.single && !first.common.albumartist.is_empty() {
            first.common.albumartist.clone()
        } else {
            first.artist.clone()
        };
        let year = first.common.year;
        let identical = old.filter(|a| {
            a.title == title
                && a.artist == artist
                && a.year == year
                && a.tracks.len() == tracks.len()
        });
        let mut local_genres = Vec::new();
        let mut notes = Vec::new();
        let mut producers = Vec::new();
        for t in &tracks {
            for g in &t.common.genres {
                if !local_genres.contains(g) {
                    local_genres.push(g.clone());
                }
            }
            for n in &t.common.comments {
                if !notes.contains(n) {
                    notes.push(n.clone());
                }
            }
            for p in &t.common.producers {
                producers.push(
                    json!({"name":p,"role":"producer","source":"local","trackTitle":t.title}),
                );
            }
        }
        if let Some(a) = identical {
            producers.extend(
                a.producers
                    .iter()
                    .filter(|p| p["source"] != "local")
                    .cloned(),
            );
        }
        Ok(Album{id,title,artist,year,disc_count:tracks.iter().map(|t|t.common.disc_total.or(t.disc_number).unwrap_or(1)).max().unwrap_or(1),
            description:identical.and_then(|a|a.description.clone()),description_source:identical.and_then(|a|a.description_source.clone()),
            introduction:identical.map(|a|a.introduction.clone()).unwrap_or(json!({"status":"unqueried"})),
            online:identical.map(|a|a.online.clone()).unwrap_or_else(||json!({"status":"unqueried","releaseId":first.common.release_id,"releaseGroupId":first.common.release_group_id})),
            local_note:if notes.is_empty(){None}else{Some(notes.join("\n"))}, genre_id:String::new(),raw_genres:local_genres.clone(),
            folder:entry.folder.clone(),tracks,producers,offline:false,root:root.into(),cover,local_genres,
            online_genres:identical.map(|a|a.online_genres.clone()).unwrap_or_default()})
    }
}

struct Entry {
    folder: PathBuf,
    tracks: Vec<PathBuf>,
    single: bool,
    cover: Option<PathBuf>,
}
impl Entry {
    fn id(&self) -> String {
        format!(
            "album-{}",
            hash(
                if self.single {
                    &self.tracks[0]
                } else {
                    &self.folder
                }
                .to_string_lossy()
                .as_bytes()
            )
        )
    }
}
fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}
fn audio(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "flac"
            | "wav"
            | "m4a"
            | "mp4"
            | "alac"
            | "dsf"
            | "dff"
            | "mp3"
            | "aac"
            | "aiff"
            | "aif"
            | "ogg"
            | "opus"
    )
}
fn walk_albums(root: &Path) -> Result<Vec<Entry>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<Entry>) -> Result<()> {
        let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| natural_key(&e.file_name().to_string_lossy()));
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for e in entries {
            let ft = e.file_type()?;
            if ft.is_file() {
                files.push(e.path())
            } else if ft.is_dir() && !e.file_name().to_string_lossy().starts_with('.') {
                dirs.push(e.path())
            }
        }
        let tracks: Vec<_> = files.iter().filter(|p| audio(p)).cloned().collect();
        let mut images: Vec<_> = files
            .iter()
            .filter(|p| matches!(extension(p).as_str(), "png" | "jpg" | "jpeg" | "webp"))
            .cloned()
            .collect();
        images.sort_by_key(|p| {
            (
                ["cover", "folder", "front"]
                    .iter()
                    .position(|s| {
                        *s == p
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_lowercase()
                    })
                    .unwrap_or(99),
                p.clone(),
            )
        });
        if dir == root {
            for file in tracks {
                let stem = normalized(&file.file_stem().unwrap_or_default().to_string_lossy());
                let cover = images
                    .iter()
                    .find(|p| {
                        normalized(&p.file_stem().unwrap_or_default().to_string_lossy()) == stem
                    })
                    .cloned();
                out.push(Entry {
                    folder: dir.into(),
                    tracks: vec![file],
                    single: true,
                    cover,
                });
            }
        } else if !tracks.is_empty() {
            out.push(Entry {
                folder: dir.into(),
                tracks,
                single: false,
                cover: images.first().cloned(),
            });
        }
        for child in dirs {
            walk(root, &child, out)?;
        }
        Ok(())
    }
    let mut entries = Vec::new();
    walk(root, root, &mut entries)?;
    Ok(entries)
}
fn file_cover(path: &PathBuf) -> Result<Cover> {
    let m = fs::metadata(path)?;
    Ok(Cover {
        path: path.clone(),
        mime: format!("image/{}", extension(path)),
        version: hash(format!(
            "{}:{}:{:?}",
            path.display(),
            m.len(),
            m.modified()?
        )),
        embedded: false,
    })
}
pub fn natural_key(value: &str) -> String {
    let mut result = String::new();
    let mut digits = String::new();
    for c in value.to_lowercase().chars().chain(std::iter::once('\0')) {
        if c.is_ascii_digit() {
            digits.push(c)
        } else {
            if !digits.is_empty() {
                result.push_str(&format!("{:0>12}", digits));
                digits.clear();
            }
            result.push(c);
        }
    }
    result
}
fn read_track(root: &Path, file: &Path, album_id: &str, fp: &str, data: &Path) -> Result<Track> {
    let ext = extension(file);
    let id = format!("track-{}", hash(file.to_string_lossy().as_bytes()));
    let mut t = Track {
        id: id.clone(),
        album_id: album_id.into(),
        title: file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        artist: "未知艺术家".into(),
        format: ext.to_uppercase(),
        path: file.into(),
        fingerprint: fp.into(),
        relative_path: file.strip_prefix(root)?.to_string_lossy().into_owned(),
        browser_playable: !matches!(ext.as_str(), "dsf" | "dff"),
        audio_url: format!("/api/audio/{id}"),
        ..Default::default()
    };
    match lofty::read_from_path(file) {
        Ok(tagged) => {
            let p = tagged.properties();
            t.duration = p.duration().as_secs_f64();
            t.sample_rate = p.sample_rate();
            t.bits_per_sample = p.bit_depth();
            t.bitrate = p.audio_bitrate().map(|n| n * 1000);
            t.number_of_channels = p.channels();
            t.lossless = Some(matches!(
                ext.as_str(),
                "flac" | "wav" | "aiff" | "aif" | "alac"
            ));
            t.codec = Some(
                match ext.as_str() {
                    "flac" => "FLAC",
                    "wav" | "aiff" | "aif" => "PCM",
                    "mp3" => "MPEG Layer III",
                    "ogg" => "Vorbis",
                    "opus" => "Opus",
                    "m4a" | "mp4" => "MP4 audio",
                    _ => &t.format,
                }
                .into(),
            );
            if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
                if let Some(s) = tag.title().filter(|s| !s.trim().is_empty()) {
                    t.title = s.into_owned();
                }
                if let Some(s) = tag
                    .artist()
                    .or_else(|| tag.get_string(&ItemKey::AlbumArtist).map(Into::into))
                {
                    t.artist = s.into_owned();
                }
                t.track_number = tag.track();
                t.disc_number = tag.disk();
                t.common.album = tag.album().map(|s| s.into_owned()).unwrap_or_default();
                t.common.albumartist = tag
                    .get_string(&ItemKey::AlbumArtist)
                    .unwrap_or_default()
                    .into();
                t.common.year = tag.year();
                t.common.disc_total = tag.disk_total();
                t.common.genres = tag
                    .get_strings(&ItemKey::Genre)
                    .map(str::to_owned)
                    .collect();
                t.common.comments = tag
                    .get_strings(&ItemKey::Comment)
                    .map(str::to_owned)
                    .collect();
                t.common.producers = tag
                    .get_strings(&ItemKey::Producer)
                    .map(str::to_owned)
                    .collect();
                t.common.release_id = tag
                    .get_string(&ItemKey::MusicBrainzReleaseId)
                    .unwrap_or_default()
                    .into();
                t.common.release_group_id = tag
                    .get_string(&ItemKey::MusicBrainzReleaseGroupId)
                    .unwrap_or_default()
                    .into();
                if let Some(p) = tag
                    .pictures()
                    .iter()
                    .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
                    .or_else(|| tag.pictures().first())
                {
                    if p.data().len() < 30 * 1024 * 1024 {
                        let version = hash(p.data());
                        let target = data.join("artwork").join(format!("{version}.img"));
                        fs::create_dir_all(target.parent().unwrap())?;
                        if !target.exists() {
                            fs::write(&target, p.data())?;
                        }
                        t.embedded_cover = Some(Cover {
                            path: target,
                            mime: p.mime_type().map(ToString::to_string).unwrap_or_default(),
                            version,
                            embedded: true,
                        });
                    }
                }
            }
        }
        Err(e) => t.metadata_error = Some(e.to_string()),
    }
    if matches!(ext.as_str(), "m4a" | "mp4" | "alac") {
        if let Ok(mut file) = fs::File::open(file) {
            if let Ok(mp4) =
                lofty::mp4::Mp4File::read_from(&mut file, lofty::config::ParseOptions::new())
            {
                let codec = mp4.properties().codec();
                t.codec = Some(format!("{codec:?}"));
                t.lossless = Some(matches!(
                    codec,
                    lofty::mp4::Mp4Codec::ALAC | lofty::mp4::Mp4Codec::FLAC
                ));
            }
        }
    }
    if t.track_number.is_none() {
        let re = regex::Regex::new(r"^(\d{1,2})[-_](\d{1,3})(?:[\s._-]|$)").unwrap();
        if let Some(c) = re.captures(&file.file_name().unwrap_or_default().to_string_lossy()) {
            t.disc_number = t.disc_number.or(c[1].parse().ok());
            t.track_number = c[2].parse().ok();
        }
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav(path: &Path) {
        let s = hound::WavSpec {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, s).unwrap();
        for _ in 0..8000 {
            w.write_sample(0_i16).unwrap()
        }
        w.finalize().unwrap();
    }
    #[test]
    fn scans_singles_albums_deletions_and_offline_cache() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("songs");
        fs::create_dir_all(root.join("Album")).unwrap();
        wav(&root.join("Single.wav"));
        wav(&root.join("Album/1-02 Song.wav"));
        wav(&root.join("Album/1-01 Song.wav"));
        let before = ["Single.wav", "Album/1-02 Song.wav", "Album/1-01 Song.wav"]
            .map(|p| (root.join(p), fs::read(root.join(p)).unwrap()));
        let mut l = Library::open(temp.path().join("data")).unwrap();
        l.config.roots = vec![root.clone()];
        l.scan(|_| {}).unwrap();
        assert_eq!(l.index.albums.len(), 2);
        let album = l.index.albums.iter().find(|a| a.title == "Album").unwrap();
        assert_eq!(album.tracks[0].track_number, Some(1));
        assert_eq!(album.tracks[0].duration, 1.0);
        for (p, b) in before {
            assert_eq!(fs::read(p).unwrap(), b);
        }
        fs::remove_file(root.join("Single.wav")).unwrap();
        l.scan(|_| {}).unwrap();
        assert_eq!(l.index.albums.len(), 1);
        fs::rename(&root, temp.path().join("unplugged")).unwrap();
        l.scan(|_| {}).unwrap();
        assert_eq!(l.index.albums.len(), 1);
        assert!(l.index.albums[0].offline);
    }
    #[test]
    fn roots_deduplicate_and_rules_respect_override() {
        assert_eq!(
            safe_roots(vec!["/a".into(), "/a/b".into(), "/a".into()]).unwrap(),
            vec![PathBuf::from("/a")]
        );
        assert!(safe_roots(vec!["relative".into()]).is_err());
        let mut rules = Rules::default();
        let album = Album {
            id: "a".into(),
            raw_genres: vec!["Mandopop".into()],
            ..Default::default()
        };
        assert_eq!(rules.resolve(&album), "mandopop");
        rules.album_overrides.insert("a".into(), "jazz".into());
        assert_eq!(rules.resolve(&album), "jazz");
    }
}
