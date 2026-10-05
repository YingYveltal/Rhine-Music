Rhine QQ connector (independent integration module)

Capabilities:
- Official QQ Music Skill 0.0.3: search, daily mix, playlist pagination.
- macOS QQMusicMac local library snapshot: read-only SQLite, preserves missing
  metadata, duplicate song occurrences and numeric song IDs.
- Explicit source, unknown metadata and completeness fields. A detailUrl is a
  webpage, never a playable audio URL. Local order/freshness is unverified until
  compared to the account's cloud snapshot.

From Rhine-Music-Native:
  ./scripts/cargo.sh test --manifest-path qq-connector/Cargo.toml
  ./scripts/cargo.sh run --manifest-path qq-connector/Cargo.toml -- local-summary
  ./scripts/cargo.sh run --manifest-path qq-connector/Cargo.toml -- search '周杰伦 晴天'
  ./scripts/cargo.sh run --manifest-path qq-connector/Cargo.toml -- daily
  ./scripts/cargo.sh run --manifest-path qq-connector/Cargo.toml -- playlist 9511461415

Official operations require QQMUSIC_API_KEY in the launching process environment.
The module does not save it. Do not add keys to code, arguments, logs or git.
The local-summary command does not require a key.

Integration entry points: official::OfficialClient and local::read_library.
Both are blocking; run on a worker, never the UI/render thread. Keep the key in
the native process. Do not return it to the frontend. No Tauri/main-app changes
are made by this module. It has its own Cargo.lock and target to avoid concurrent
visual-development build conflicts. The temporary Python test page calls the
binary; the application should call the Rust library directly.

Limitations: no audio-stream API in the official Skill, no playback implementation,
no persistent login, no proven arbitrary-song native-client control interface.
See ../verification/qq-connect/integration-report.json and NOTES.txt for evidence.
