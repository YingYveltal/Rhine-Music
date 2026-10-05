//! Call blocking methods on a worker thread, never the WebView/render thread.
//! Official protocol: tencentmusic/qqmusic-skills, qqmusic/SKILL.md v0.0.3.
pub mod local;
pub mod official;
pub mod web;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: Option<String>,
    pub mid: Option<String>,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration: Option<u32>,
    pub detail_url: Option<String>,
    pub source: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub total: usize,
    pub complete: bool,
    pub pages: usize,
    pub tracks: Vec<Track>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    MissingKey,
    InvalidInput,
    Network,
    Http(u16),
    Business,
    InvalidResponse,
    PaginationStalled,
    PlaylistChanged,
    PageLimit,
    LocalDatabase,
    AccountRequired,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No upstream text, request headers, credentials or account paths.
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
