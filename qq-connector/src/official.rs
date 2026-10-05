use crate::{Error, Playlist, Track};
use reqwest::{blocking::Client, header::{HeaderMap, HeaderValue, AUTHORIZATION}};
use serde_json::{json, Value};
use std::{collections::HashSet, io::Read, sync::Mutex, time::{Duration, Instant}};

pub const SKILL_VERSION: &str = "0.0.3";
const BASE: &str = "https://a.y.qq.com";
const MAX_BODY: u64 = 4 * 1024 * 1024;

pub struct OfficialClient {
    http: Client,
    last_request: Mutex<Option<Instant>>,
}
impl std::fmt::Debug for OfficialClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OfficialClient { credentials: REDACTED }")
    }
}
impl OfficialClient {
    pub fn new(key: &str) -> Result<Self, Error> {
        if key.is_empty() { return Err(Error::MissingKey); }
        let mut auth = HeaderValue::from_str(&format!("Bearer {key}")).map_err(|_| Error::InvalidInput)?;
        auth.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);
        let http = Client::builder().default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(8)).timeout(Duration::from_secs(20))
            .build().map_err(|_| Error::Network)?;
        Ok(Self { http, last_request: Mutex::new(None) })
    }

    fn request(&self, path: &str, params: Value) -> Result<Value, Error> {
        // Paths are private constants: callers cannot send a key to a custom URL.
        let mut last = self.last_request.lock().map_err(|_| Error::Network)?;
        if let Some(t) = *last {
            std::thread::sleep(Duration::from_millis(750).saturating_sub(t.elapsed()));
        }
        *last = Some(Instant::now());
        let response = self.http.post(format!("{BASE}{path}"))
            .json(&envelope(params)).send().map_err(|_| Error::Network)?;
        if !response.status().is_success() { return Err(Error::Http(response.status().as_u16())); }
        let mut bytes = Vec::new();
        response.take(MAX_BODY + 1).read_to_end(&mut bytes).map_err(|_| Error::Network)?;
        if bytes.len() as u64 > MAX_BODY { return Err(Error::InvalidResponse); }
        let data: Value = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidResponse)?;
        check_business(&data)?;
        Ok(data)
    }

    pub fn search(&self, query: &str, page: u32) -> Result<(Vec<Track>, bool), Error> {
        if query.trim().is_empty() || query.chars().count() > 100 { return Err(Error::InvalidInput); }
        let data = self.request("/discover/search", json!({"keyword":query,"type":"0","page":page}))?;
        let songs = data["songs"].as_array().ok_or(Error::InvalidResponse)?;
        Ok((songs.iter().map(normalize).collect(), data["hasMore"].as_bool().ok_or(Error::InvalidResponse)?))
    }

    pub fn daily(&self) -> Result<Vec<Track>, Error> {
        let data = self.request("/discover/daily-mix", json!({}))?;
        Ok(data["songlist"].as_array().ok_or(Error::InvalidResponse)?.iter().map(normalize).collect())
    }

    pub fn playlist(&self, id: u64) -> Result<Playlist, Error> {
        collect_playlist(id, |page| self.request("/playlists/detail", json!({"dissId":id,"page":page})))
    }
}

fn envelope(params: Value) -> Value { json!({"params":params,"comm":{"skill_version":SKILL_VERSION}}) }

fn check_business(data: &Value) -> Result<(), Error> {
    if !data.is_object() { return Err(Error::InvalidResponse); }
    for name in ["ret", "sub_ret"] {
        if let Some(value) = data.get(name).filter(|v| !v.is_null()) {
            if value.as_i64() != Some(0) { return Err(Error::Business); }
        }
    }
    Ok(())
}

fn normalize(value: &Value) -> Track {
    let mid = value["songMid"].as_str().filter(|s| !s.is_empty()).map(str::to_owned);
    let detail_url = mid.as_ref().filter(|m| m.bytes().all(|b| b.is_ascii_alphanumeric()))
        .map(|m| format!("https://i2.y.qq.com/a/song/{m}"));
    Track { id: None, mid, title: value["songName"].as_str().unwrap_or("").to_owned(),
        artists: value["singerName"].as_str().filter(|s| !s.is_empty()).into_iter().map(str::to_owned).collect(),
        album: None, duration: None, detail_url, source: "qq-official".into() }
}

fn collect_playlist(id: u64, mut fetch: impl FnMut(u32) -> Result<Value, Error>) -> Result<Playlist, Error> {
    let mut tracks = Vec::new();
    let mut seen = HashSet::new();
    let mut expected = None;
    for page in 0..40 {
        let data = fetch(page)?;
        let total = data["totalNum"].as_u64().ok_or(Error::InvalidResponse)? as usize;
        if expected.is_some_and(|n| n != total) { return Err(Error::PlaylistChanged); }
        expected = Some(total);
        let batch = data["trackList"].as_array().ok_or(Error::InvalidResponse)?;
        let more = data["hasMore"].as_bool().ok_or(Error::InvalidResponse)?;
        if !batch.is_empty() && !seen.insert(serde_json::to_string(batch).map_err(|_| Error::InvalidResponse)?) {
            return Err(Error::PaginationStalled);
        }
        tracks.extend(batch.iter().map(normalize));
        if !more {
            return Ok(Playlist { id: id.to_string(), name: data["dissName"].as_str().unwrap_or("").to_owned(),
                total, complete: tracks.len() == total, pages: page as usize + 1, tracks, source: "qq-official".into() });
        }
        if batch.is_empty() { return Err(Error::PaginationStalled); }
    }
    Err(Error::PageLimit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_and_metadata_contract() {
        assert_eq!(envelope(json!({})), json!({"params":{},"comm":{"skill_version":"0.0.3"}}));
        assert_eq!(check_business(&json!({"ret":0,"sub_ret":9})), Err(Error::Business));
        let track = normalize(&json!({"songMid":"abc","songName":"demo","songH5Url":"https://evil.invalid"}));
        assert_eq!(track.detail_url.as_deref(),Some("https://i2.y.qq.com/a/song/abc"));
        assert!(track.album.is_none() && track.duration.is_none());
    }
    #[test]
    fn paginates_without_dropping_duplicate_occurrences() {
        let out = collect_playlist(7, |page| Ok(if page == 0 {
            json!({"totalNum":3,"trackList":[{"songMid":"a"},{"songMid":"a"}],"hasMore":true})
        } else { json!({"totalNum":3,"trackList":[{"songMid":"b"}],"hasMore":false}) })).unwrap();
        assert!(out.complete);
        assert_eq!(out.pages,2);
        assert_eq!(out.tracks.len(),3);
    }
    #[test]
    fn incomplete_and_stalled_are_not_complete() {
        let out=collect_playlist(7, |_| Ok(json!({"totalNum":5,"trackList":[],"hasMore":false}))).unwrap();
        assert!(!out.complete);
        assert!(matches!(collect_playlist(7, |_| Ok(json!({"totalNum":5,"trackList":[{"songMid":"a"}],"hasMore":true}))), Err(Error::PaginationStalled)));
    }
}
