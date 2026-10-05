use super::*;
use rhine_qq_connector::web::collect_album;

struct NoCredentials;
impl crate::qq_session::CredentialStore for NoCredentials {
    fn load(&mut self) -> Result<Option<Credentials>> { panic!("no credentials in pagination tests") }
    fn save(&mut self, _: &Credentials) -> Result<()> { panic!("no credentials in pagination tests") }
    fn delete(&mut self) -> Result<()> { panic!("no credentials in pagination tests") }
}
fn page(ids: impl IntoIterator<Item = u64>, total: u64) -> Value {
    // No artwork URL or album.mid: the production cover path cannot make HTTP requests.
    json!({"totalNum":total,"songList":ids.into_iter().map(|id|json!({"songInfo":{
        "id":id,"mid":format!("SYNTHETIC{id}"),"title":format!("Track {id}")
    }})).collect::<Vec<_>>()})
}

#[test]
fn inconsistent_album_sync_keeps_disk_and_memory_snapshot_then_retries_successfully() {
    let failures = vec![
        ("总数发生变化", vec![Ok(page(0..100, 150)), Ok(page([], 100))]),
        ("总数发生变化", vec![Ok(page(0..100, 150)), Ok(page(100..200, 200))]),
        ("数量不一致", vec![Ok(page(0..100, 150)), Ok(page(0..100, 150))]),
        ("未取全", vec![Ok(page(0..100, 150)), Ok(page([], 150))]),
        ("synthetic network failure", vec![Ok(page(0..100, 150)), Err(anyhow::anyhow!("synthetic network failure"))]),
    ];
    for (message, pages) in failures {
        let dir = tempfile::tempdir().unwrap();
        let connection = Connection::open(dir.path(), Box::new(NoCredentials), None).unwrap();
        let (album, songs) = make_album("qq-old", "Previous library", "qq-albums",
            &[json!({"mid":"OLD","title":"Old track"})], None);
        let saved = Saved { albums: vec![album], songs, updated: "previous sync".into(), enabled: true };
        let path = dir.path().join("library.json");
        atomic_json(&path, &saved).unwrap();
        let original_disk = std::fs::read(&path).unwrap();
        let original_memory = serde_json::to_value(&saved).unwrap();
        let qq = Qq { connection, saved: Mutex::new(saved), job: Mutex::new(json!({})), root: dir.path().into() };
        let generation = qq.connection.account_snapshot().0;
        let directory = json!({"mydiss":{"list":[]}});
        let albums = [json!({"albummid":"ready","albumname":"First staged album"}),
            json!({"albummid":"target","albumname":"Paginated album"})];
        let mut staged_first = false;
        let mut pages = pages.into_iter();
        let error = qq.sync_entries(generation, &directory, &[], &albums,
            |id, liked| { assert_eq!(id, 0); assert!(liked); Ok(json!({"tracks":[]})) },
            |mid| {
                if mid == "ready" {
                    staged_first = true;
                    return Ok(vec![json!({"mid":"READY","title":"Staged track"})]);
                }
                assert_eq!(mid, "target");
                collect_album(|_| pages.next().expect("unexpected page request"))
            }).unwrap_err();
        assert!(staged_first, "failure must occur after partial new-library staging");
        assert!(pages.next().is_none());
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(std::fs::read(&path).unwrap(), original_disk, "failed sync changed the on-disk library");
        assert_eq!(serde_json::to_value(&*qq.saved.lock().unwrap()).unwrap(), original_memory,
            "failed sync changed the in-memory library");

        // Retry the same production staging/commit path with consistent pages,
        // including an entirely repeated page and repeated final-page entries.
        let mut pages = [page(0..100, 203), page(0..100, 203), page([5, 0, 5], 203)].into_iter();
        let total = qq.sync_entries(generation, &directory, &[], &albums,
            |_, _| Ok(json!({"tracks":[]})),
            |mid| {
                if mid == "ready" { return Ok(vec![json!({"mid":"READY","title":"Staged track"})]); }
                collect_album(|_| Ok(pages.next().expect("unexpected page request")))
            }).unwrap();
        assert_eq!(total, 3); // liked list plus two albums
        assert!(pages.next().is_none());
        let saved = qq.saved.lock().unwrap();
        assert!(saved.enabled);
        assert_ne!(saved.updated, "previous sync");
        assert_eq!(saved.albums.iter().map(|album|album.id.as_str()).collect::<Vec<_>>(),
            ["qq-playlist-liked", "qq-album-ready", "qq-album-target"]);
        let tracks = &saved.albums[2].tracks;
        let expected = (0..100).chain(0..100).chain([5, 0, 5]).map(|id|format!("Track {id}")).collect::<Vec<_>>();
        assert_eq!(tracks.iter().map(|track|track.title.clone()).collect::<Vec<_>>(), expected);
        assert_eq!(tracks.iter().map(|track|&track.id).collect::<HashSet<_>>().len(), 203);
        assert_eq!(saved.songs.len(), 204); // Occurrences, not unique song mids.
        let disk: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(disk, serde_json::to_value(&*saved).unwrap());
    }
}
