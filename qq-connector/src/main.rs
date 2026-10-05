use rhine_qq_connector::{Error, local, official::OfficialClient};
use serde_json::{json, Value};

fn run(args: &[String]) -> Result<Value, Error> {
    if args.first().map(String::as_str) == Some("local-summary") {
        let library=local::read_library(&local::default_path()?,None)?;
        return Ok(json!({"source":"qq-local-cache", "personalFolders":library.len(),
          "likedTracks":library.iter().filter(|p|p.liked).map(|p|p.tracks.len()).sum::<usize>(),
          "playlistEntries":library.iter().map(|p|p.tracks.len()).sum::<usize>(),
          "foldersWithExpectedCount":library.iter().filter(|p|p.cached_count_matches).count(),
          "cloudFreshnessVerified":false,"playbackTested":false}));
    }
    let key=std::env::var("QQMUSIC_API_KEY").map_err(|_|Error::MissingKey)?;
    let client=OfficialClient::new(&key)?;
    match args.first().map(String::as_str) {
        Some("search") => {
            let query=args.get(1).ok_or(Error::InvalidInput)?;
            let (tracks,has_more)=client.search(query,0)?;
            Ok(json!({"source":"qq-official","tracks":tracks,"hasMore":has_more}))
        }
        Some("daily") => Ok(json!({"source":"qq-official","tracks":client.daily()?})),
        Some("playlist") => Ok(serde_json::to_value(client.playlist(args.get(1).ok_or(Error::InvalidInput)?
            .parse().map_err(|_|Error::InvalidInput)?)?).map_err(|_|Error::InvalidResponse)?),
        _ => Err(Error::InvalidInput),
    }
}
fn main() {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(value) => println!("{value}"),
        Err(error) => { eprintln!("QQ connector: {error}"); std::process::exit(1); }
    }
}
