use crate::library::{normalized, timestamp, Album};
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub struct Online {
    client: Client,
    last: Option<Instant>,
}
impl Online {
    pub fn new(contact: &str) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .user_agent(format!(
                    "RhineMusicNative/0.1 (Rhine Music local music player; {})",
                    if contact.is_empty() {
                        "local desktop client"
                    } else {
                        contact
                    }
                ))
                .timeout(Duration::from_secs(8))
                .build()?,
            last: None,
        })
    }
    fn get(&mut self, url: &str, params: &[(&str, String)]) -> Result<Value> {
        if let Some(last) = self.last {
            let remaining = Duration::from_millis(1100).saturating_sub(last.elapsed());
            if !remaining.is_zero() {
                std::thread::sleep(remaining)
            }
        }
        self.last = Some(Instant::now());
        let response = self
            .client
            .get(url)
            .query(params)
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        if !response["error"].is_null() {
            bail!("资料服务返回错误：{}", response["error"])
        }
        Ok(response)
    }
    pub fn introduction(&mut self, album: &mut Album) -> Result<()> {
        let outcome = self.find_introduction(album);
        match outcome {
            Ok(Some((text, source))) => {
                album.description = Some(text);
                album.description_source = Some(source);
                album.introduction = json!({"status":"matched","checkedAt":timestamp()});
                Ok(())
            }
            Ok(None) => {
                album.introduction = json!({"status":"not-found","checkedAt":timestamp()});
                Ok(())
            }
            Err(e) => {
                album.introduction =
                    json!({"status":"error","checkedAt":timestamp(),"error":e.to_string()});
                Err(e)
            }
        }
    }
    fn find_introduction(&mut self, album: &Album) -> Result<Option<(String, Value)>> {
        let mut successes = 0;
        let mut errors = Vec::new();
        for language in ["zh", "en"] {
            let url = format!("https://{language}.wikipedia.org/w/api.php");
            let query = format!("{} {}", album.title, album.artist);
            let search = match self.get(
                &url,
                &[
                    ("action", "query".into()),
                    ("format", "json".into()),
                    ("list", "search".into()),
                    ("srsearch", query),
                    ("srlimit", "5".into()),
                ],
            ) {
                Ok(v) => {
                    successes += 1;
                    v
                }
                Err(e) => {
                    errors.push(e.to_string());
                    continue;
                }
            };
            let Some(hits) = search["query"]["search"].as_array() else {
                continue;
            };
            let mut confirmed = Vec::new();
            for hit in hits {
                let page_id = hit["pageid"].as_u64().context("百科页面缺少 ID")?;
                let page = self.get(
                    &url,
                    &[
                        ("action", "query".into()),
                        ("format", "json".into()),
                        ("pageids", page_id.to_string()),
                        ("prop", "extracts|pageprops|info".into()),
                        ("exintro", "1".into()),
                        ("explaintext", "1".into()),
                        ("inprop", "url".into()),
                    ],
                )?;
                let page = &page["query"]["pages"][page_id.to_string()];
                if !page["pageprops"]["disambiguation"].is_null() {
                    continue;
                }
                let text = page["extract"].as_str().unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                let Some(qid) = page["pageprops"]["wikibase_item"].as_str() else {
                    continue;
                };
                let entity = self.get(
                    "https://www.wikidata.org/w/api.php",
                    &[
                        ("action", "wbgetentities".into()),
                        ("format", "json".into()),
                        ("ids", qid.into()),
                        ("props", "claims|labels|aliases".into()),
                        ("languages", "zh|zh-hans|zh-hant|en".into()),
                    ],
                )?;
                let entity = &entity["entities"][qid];
                if !confirmed_album(album, page, entity, text) {
                    continue;
                }
                // Performer identity is checked against Wikidata names, not search snippets.
                let performer_ids = claim_ids(entity, "P175");
                if performer_ids.is_empty() {
                    continue;
                }
                let performers = self.get(
                    "https://www.wikidata.org/w/api.php",
                    &[
                        ("action", "wbgetentities".into()),
                        ("format", "json".into()),
                        ("ids", performer_ids.join("|")),
                        ("props", "labels|aliases".into()),
                        ("languages", "zh|zh-hans|zh-hant|en".into()),
                    ],
                )?;
                let artist = normalized(&album.artist);
                let artist_matches = performers["entities"]
                    .as_object()
                    .map(|e| {
                        e.values()
                            .any(|v| names(v).iter().any(|n| normalized(n) == artist))
                    })
                    .unwrap_or(false);
                if !artist_matches {
                    continue;
                }
                let source = page["fullurl"].as_str().unwrap_or("");
                if source.is_empty() {
                    continue;
                }
                confirmed.push((text.to_owned(),json!({"name":if language=="zh"{"维基百科"}else{"Wikipedia"},"url":source,"checkedAt":timestamp(),"license":"CC BY-SA · Wikipedia"})));
            }
            if confirmed.len() == 1 {
                return Ok(confirmed.pop());
            }
            if confirmed.len() > 1 {
                return Ok(None);
            }
        }
        if successes == 0 {
            bail!("{}", errors.join("；"))
        }
        Ok(None)
    }
    pub fn enrich(&mut self, album: &mut Album) -> Result<()> {
        let result = self.enrich_inner(album);
        if let Err(e) = &result {
            album.online = json!({"status":"error","error":e.to_string(),"checkedAt":timestamp()});
        }
        result
    }
    fn enrich_inner(&mut self, album: &mut Album) -> Result<()> {
        let existing = album.online["releaseId"]
            .as_str()
            .filter(|s| valid_mbid(s))
            .map(str::to_owned);
        let id = if let Some(id) = existing {
            id
        } else {
            let escape = |s: &str| s.replace('"', "\\\"");
            let q = format!(
                "release:\"{}\" AND artist:\"{}\"",
                escape(&album.title),
                escape(&album.artist)
            );
            let results = self.get(
                "https://musicbrainz.org/ws/2/release/",
                &[("query", q), ("fmt", "json".into()), ("limit", "10".into())],
            )?;
            let candidates = results["releases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| release_matches(album, r))
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                album.online = json!({"status":if candidates.is_empty(){"not-found"}else{"uncertain"},"checkedAt":timestamp()});
                return Ok(());
            }
            candidates[0]["id"]
                .as_str()
                .context("MusicBrainz 缺少发行 ID")?
                .to_owned()
        };
        let release = self.get(
            &format!("https://musicbrainz.org/ws/2/release/{id}"),
            &[
                (
                    "inc",
                    "artist-credits+recordings+release-groups+genres+artist-rels+recording-rels"
                        .into(),
                ),
                ("fmt", "json".into()),
            ],
        )?;
        let group_id = release["release-group"]["id"].as_str().unwrap_or("");
        let group = if valid_mbid(group_id) {
            self.get(
                &format!("https://musicbrainz.org/ws/2/release-group/{group_id}"),
                &[
                    ("inc", "genres+artist-rels+url-rels".into()),
                    ("fmt", "json".into()),
                ],
            )?
        } else {
            Value::Null
        };
        album.online_genres = release["genres"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(group["genres"].as_array().into_iter().flatten())
            .filter_map(|g| g["name"].as_str().map(str::to_owned))
            .collect();
        album.online_genres.sort();
        album.online_genres.dedup();
        album.producers.retain(|p| p["source"] != "MusicBrainz");
        for record in [&release, &group] {
            credits(record, None, &mut album.producers);
        }
        for media in release["media"].as_array().into_iter().flatten() {
            for track in media["tracks"].as_array().into_iter().flatten() {
                credits(
                    &track["recording"],
                    track["title"].as_str(),
                    &mut album.producers,
                );
            }
        }
        album.online = json!({"status":"matched","releaseId":id,"releaseGroupId":group_id,"sourceUrl":format!("https://musicbrainz.org/release/{id}"),"checkedAt":timestamp()});
        Ok(())
    }
}
fn valid_mbid(s: &str) -> bool {
    s.len() == 36
        && s.chars().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
fn names(entity: &Value) -> Vec<String> {
    entity["labels"]
        .as_object()
        .into_iter()
        .flat_map(|o| o.values())
        .chain(
            entity["aliases"]
                .as_object()
                .into_iter()
                .flat_map(|o| o.values())
                .flat_map(|v| v.as_array().into_iter().flatten()),
        )
        .filter_map(|v| v["value"].as_str().map(str::to_owned))
        .collect()
}
fn claim_ids(entity: &Value, property: &str) -> Vec<String> {
    entity["claims"][property]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            c["mainsnak"]["datavalue"]["value"]["id"]
                .as_str()
                .map(str::to_owned)
        })
        .collect()
}
fn confirmed_album(album: &Album, page: &Value, entity: &Value, _text: &str) -> bool {
    let classes = claim_ids(entity, "P31");
    if !classes.iter().any(|s| {
        matches!(
            s.as_str(),
            "Q482994" | "Q208569" | "Q414474" | "Q191849" | "Q253868" | "Q169930"
        )
    }) {
        return false;
    }
    let base_title = page["title"]
        .as_str()
        .unwrap_or("")
        .split(['(', '（'])
        .next()
        .unwrap_or("");
    if normalized(base_title) != normalized(&album.title)
        && !names(entity)
            .iter()
            .any(|n| normalized(n) == normalized(&album.title))
    {
        return false;
    }
    if let Some(year) = album.year {
        let years = entity["claims"]["P577"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["mainsnak"]["datavalue"]["value"]["time"].as_str())
            .filter_map(|t| t.trim_start_matches('+').get(..4)?.parse::<u32>().ok())
            .collect::<Vec<_>>();
        if !years.is_empty() && !years.contains(&year) {
            return false;
        }
    }
    true
}
fn release_matches(album: &Album, r: &Value) -> bool {
    if normalized(r["title"].as_str().unwrap_or("")) != normalized(&album.title) {
        return false;
    }
    let artist = r["artist-credit"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| {
            a["name"]
                .as_str()
                .or(a["artist"]["name"].as_str())
                .unwrap_or("")
        })
        .collect::<String>();
    if normalized(&artist) != normalized(&album.artist) {
        return false;
    }
    if let Some(year) = album.year {
        if let Some(date) = r["date"].as_str() {
            if date.get(..4).and_then(|v| v.parse::<u32>().ok()) != Some(year) {
                return false;
            }
        }
    }
    let count = r["track-count"].as_u64();
    count.is_none_or(|n| n == album.tracks.len() as u64)
}
fn credits(record: &Value, track: Option<&str>, out: &mut Vec<Value>) {
    for r in record["relations"].as_array().into_iter().flatten() {
        let role = r["type"].as_str().unwrap_or("");
        if matches!(
            role,
            "producer"
                | "executive producer"
                | "engineer"
                | "mix"
                | "mastering"
                | "arranger"
                | "composer"
        ) {
            if let Some(name) = r["artist"]["name"].as_str() {
                let v = json!({"name":name,"role":role,"source":"MusicBrainz","trackTitle":track,"url":format!("https://musicbrainz.org/artist/{}",r["artist"]["id"].as_str().unwrap_or(""))});
                if !out.contains(&v) {
                    out.push(v);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_wrong_work_and_year() {
        let a = Album {
            title: "Example".into(),
            artist: "Artist".into(),
            year: Some(2020),
            ..Default::default()
        };
        let p = json!({"title":"Example (album)"});
        let mut e = json!({"claims":{"P31":[{"mainsnak":{"datavalue":{"value":{"id":"Q482994"}}}}],"P577":[{"mainsnak":{"datavalue":{"value":{"time":"+2020-01-01T00:00:00Z"}}}}]}});
        assert!(confirmed_album(&a, &p, &e, ""));
        e["claims"]["P31"][0]["mainsnak"]["datavalue"]["value"]["id"] = json!("Q7366");
        assert!(!confirmed_album(&a, &p, &e, ""));
    }
}
