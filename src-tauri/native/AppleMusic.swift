import Foundation
import MusicKit
import CryptoKit
import ImageIO

// Validate downloaded/cached artwork before exposing it to all frontend views.
@_cdecl("rhine_apple_artwork_valid")
func rhineAppleArtworkValid(_ bytes: UnsafePointer<UInt8>, _ count: Int) -> UInt8 {
    let data = Data(bytes: bytes, count: count)
    guard let source = CGImageSourceCreateWithData(data as CFData, nil),
          let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return 0 }
    guard image.width > 0, image.height > 0, image.width <= 4096, image.height <= 4096 else { return 0 }
    switch image.alphaInfo {
    case .none, .noneSkipFirst, .noneSkipLast: return 1
    default: break
    }
    // MusicKit can return a transparent image for an artwork-less playlist.
    // Reject only zero-alpha images, not plain but opaque artwork.
    var alpha = [UInt8](repeating: 0, count: image.width * image.height * 4)
    let drawn = alpha.withUnsafeMutableBytes { pixels -> Bool in
        guard let context = CGContext(data: pixels.baseAddress, width: image.width, height: image.height,
            bitsPerComponent: 8, bytesPerRow: image.width * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else { return false }
        context.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return true
    }
    return drawn && stride(from: 3, to: alpha.count, by: 4).contains(where: { alpha[$0] != 0 }) ? 1 : 0
}

private func failure(_ message: String) -> NSError {
    NSError(domain: "RhineApple", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
}
private func safeError(_ error: Error) -> String {
    let e = error as NSError
    return e.domain == "RhineApple" ? e.localizedDescription : "Apple Music: \(e.domain) / \(e.code)"
}
private func digest(_ fields: [String]) -> String {
    let bytes = try! JSONSerialization.data(withJSONObject: fields)
    return SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
}

// A zero/zero response can be a real empty playlist or a relationship that
// has not arrived yet. Re-read once, without treating repeated emptiness as proof.
func retryEmptyPlaylistRead<Entry, Item>(
    read: () async throws -> (entries: [Entry], tracks: [Item]),
    reread: () async throws -> (entries: [Entry], tracks: [Item])
) async throws -> (entries: [Entry], tracks: [Item]) {
    let result = try await read()
    if result.entries.isEmpty && result.tracks.isEmpty { return try await reread() }
    return result
}

// Pagination is atomic: a missing promised page never becomes a partial success.
func readApplePages<T>(initial: [T], hasNext: Bool,
    next: () async throws -> (items: [T], hasNext: Bool)?) async throws -> [T] {
    var result = initial, more = hasNext
    while more {
        guard let page = try await next(), !page.items.isEmpty else {
            throw failure("Apple Music 分页未完成，已保留上次完整曲库")
        }
        result.append(contentsOf: page.items); more = page.hasNext
    }
    return result
}

func retryEmptyAlbumRead<T>(read: () async throws -> [T], reread: () async throws -> [T]) async throws -> [T] {
    let tracks = try await read()
    return tracks.isEmpty ? try await reread() : tracks
}

struct AppleAlbumTrackIdentity {
    let id: String
    let disc: Int?
    let number: Int?
}
// Missing numbering cannot establish a different order. Keep the source order
// in that case; otherwise sort by the real disc/track pair, stably on ties.
func appleAlbumOrder(_ tracks: [AppleAlbumTrackIdentity]) -> [Int] {
    let indices = Array(tracks.indices)
    guard tracks.allSatisfy({ ($0.disc ?? 0) > 0 && ($0.number ?? 0) > 0 }) else { return indices }
    return indices.sorted {
        let a = tracks[$0], b = tracks[$1]
        return (a.disc!, a.number!, $0) < (b.disc!, b.number!, $1)
    }
}
func appleAlbumIdentity(_ libraryID: String, _ tracks: [AppleAlbumTrackIdentity]) -> (id: String, revision: String, occurrences: [String]) {
    let id = "apple-album-" + digest([libraryID])
    let revision = digest(tracks.enumerated().map { "\($0.offset):\($0.element.id):\($0.element.disc.map(String.init) ?? ""):\($0.element.number.map(String.init) ?? "")" })
    return (id, revision, tracks.enumerated().map { "apple-track-album-" + digest([id, revision, $0.element.id, String($0.offset)]) })
}
func appleReleaseYear(_ date: Date?) -> Int? {
    guard let date else { return nil }
    var calendar = Calendar(identifier: .gregorian); calendar.timeZone = TimeZone(secondsFromGMT: 0)!
    return calendar.component(.year, from: date)
}

// The queue omits unsupported media, but library occurrences keep their original positions.
func appleAudioQueuePlan(_ supported: [Bool], selected: Int) throws -> (indices: [Int], selected: Int) {
    guard supported.indices.contains(selected), supported[selected] else { throw failure("暂不支持此类型的资料库条目") }
    let indices = supported.indices.filter { supported[$0] }
    return (indices, indices.firstIndex(of: selected)!)
}
@MainActor func startVerifiedAppleQueue(prepare: () async throws -> Void,
    canStart: () -> Bool, verify: () throws -> Void, play: () async throws -> Void) async throws {
    try await prepare()
    guard canStart() else { throw failure("播放操作已取消") }
    try verify()
    try await play()
}

@available(macOS 14.0, *)
@MainActor private final class AppleBridge {
    static let shared = AppleBridge()
    struct Item { let track: Track; let json: [String: Any]; let supported: Bool }
    var items: [String: Item] = [:]
    var albums: [[String: Any]] = []
    var syncing = false
    var completed = 0
    var total = 0
    var player: ApplicationMusicPlayer { .shared }
    var generation: UInt64 = 0
    var queueGeneration: UInt64 = 0
    var inFlight = 0
    var stopping = 0
    var intent = "idle"
    var queue: [Item] = []
    var runtimeIDs: [String] = []
    var lastIndex = 0
    var originalIndices: [Int] = []

    func verifyQueue() throws {
        let observed = Array(player.queue.entries)
        guard observed.count == queue.count,
            Set(observed.map(\.id)).count == observed.count,
            zip(observed, queue).allSatisfy({ $0.title == ($1.json["title"] as? String) }) else { throw failure("原生队列身份无法核对") }
        let ids = observed.map(\.id)
        guard let current = player.queue.currentEntry, ids.firstIndex(of: current.id) == lastIndex else {
            throw failure("原生播放器未定位到所选资料库条目")
        }
        runtimeIDs = ids
    }
    // play() can finish before its native state notification. On failure, wait
    // through the observed late transition instead of acknowledging one stale pause.
    func stopFailedOperation(_ ticket: UInt64) async throws {
        let deadline = Date().addingTimeInterval(15)
        var quietSince: Date? = nil
        while true {
            guard ticket == generation else { return }
            player.stop()
            let now = Date()
            if player.state.playbackStatus == .playing { quietSince = nil }
            else if let quietSince, now.timeIntervalSince(quietSince) >= 0.25 { return }
            else if quietSince == nil { quietSince = now }
            guard now < deadline else { throw failure("Apple Music 尚未确认停止") }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
    }

    func authorization() -> String { MusicAuthorization.currentStatus.rawValue }
    func checkAuthorization() throws {
        guard MusicAuthorization.currentStatus == .authorized else { throw failure("请先允许 Rhine 访问音乐资料库") }
    }
    func all<T: MusicItem>(_ initial: MusicItemCollection<T>) async throws -> [T] {
        var batch = initial
        return try await readApplePages(initial: Array(initial), hasNext: initial.hasNextBatch) {
            guard let next = try await batch.nextBatch(limit: 100) else { return nil }
            batch = next
            return (Array(next), next.hasNextBatch)
        }
    }

    func artwork(_ body: [String: Any]) async throws -> [String: Any] {
        guard let raw = body["url"] as? String, let url = URL(string: raw),
              url.scheme?.lowercased() == "musickit" else { throw failure("封面地址不可用") }
        // MusicKit's local artwork URLs are supported by the shared native
        // session, not a new ephemeral session or Rust's HTTP client.
        let (data, response) = try await URLSession.shared.data(for: URLRequest(url: url, timeoutInterval: 12))
        guard let response = response as? HTTPURLResponse, (200..<300).contains(response.statusCode),
              !data.isEmpty, data.count < 10_000_000 else { throw failure("封面暂不可用") }
        return ["data": data.base64EncodedString()]
    }
    func sync() async throws -> [[String: Any]] {
        try checkAuthorization()
        guard !syncing else { throw failure("资料库正在同步") }
        syncing = true; completed = 0; total = 0
        defer { syncing = false }
        var request = MusicLibraryRequest<Playlist>(); request.limit = 100
        let response = try await request.response()
        let playlists = try await all(response.items)
        var albumRequest = MusicLibraryRequest<Album>(); albumRequest.limit = 100
        let libraryAlbums = try await all(albumRequest.response().items)
        total = playlists.count + libraryAlbums.count
        var nextItems: [String: Item] = [:], nextAlbums: [[String: Any]] = []
        for playlist in playlists {
            let contents = try await retryEmptyPlaylistRead(read: {
                let e = try await playlist.with(.entries, preferredSource: .library)
                let t = try await playlist.with(.tracks, preferredSource: .library)
                guard let entryCollection = e.entries, let trackCollection = t.tracks else {
                    throw failure("歌单内容暂时不可用，已保留上次完整曲库")
                }
                return (try await all(entryCollection), try await all(trackCollection))
            }, reread: {
                var freshRequest = MusicLibraryRequest<Playlist>(); freshRequest.limit = 1
                freshRequest.filter(matching: \.id, equalTo: playlist.id)
                let response = try await freshRequest.response()
                guard let fresh = response.items.first, fresh.id == playlist.id else {
                    throw failure("歌单已变化，已保留上次完整曲库；请稍后重新同步")
                }
                let full = try await fresh.with(.entries, .tracks, preferredSource: .library)
                guard let entryCollection = full.entries, let trackCollection = full.tracks else {
                    throw failure("歌单内容暂时不可用，已保留上次完整曲库")
                }
                return (try await all(entryCollection), try await all(trackCollection))
            })
            let entries = contents.entries, tracks = contents.tracks
            guard entries.count == tracks.count,
                  zip(entries, tracks).allSatisfy({ $0.title == $1.title && $0.artistName == $1.artistName }) else {
                throw failure("歌单顺序无法核对，已保留上次完整曲库")
            }
            let albumID = "apple-playlist-" + digest([playlist.id.rawValue])
            let revision = digest(entries.enumerated().map { "\($0.offset):\($0.element.id.rawValue):\($0.element.item?.id.rawValue ?? "")" })
            var rows: [[String: Any]] = []
            for (i, entry) in entries.enumerated() {
                let id = "apple-track-" + digest([albumID, revision, entry.id.rawValue, String(i)])
                let supported: Bool
                if case .song = tracks[i] { supported = true } else { supported = false }
                let row: [String: Any] = ["id": id, "albumId": albumID, "source": "apple", "title": entry.title,
                    "artist": entry.artistName, "trackNumber": i + 1, "duration": entry.duration ?? 0,
                    "format": "Apple Music", "browserPlayable": false, "audioUrl": "", "relativePath": "",
                    "sourcePosition": i, "snapshotRevision": revision]
                rows.append(row); nextItems[id] = Item(track: tracks[i], json: row, supported: supported)
            }
            var album: [String: Any] = ["id": albumID, "source": "apple", "kind": "playlist", "title": playlist.name,
                "artist": playlist.curatorName ?? "Apple Music", "genreId": "apple-playlists", "rawGenres": [],
                "folder": "", "tracks": rows, "producers": [], "offline": false, "complete": !entries.isEmpty,
                "loadError": entries.isEmpty
                    ? "本次未读取到歌曲。若这本来是空歌单可忽略；若已有歌曲，请稍后重新同步。" as Any
                    : NSNull(), "snapshotRevision": revision]
            if let url = playlist.artwork?.url(width: 600, height: 600) { album["artworkURL"] = url.absoluteString }
            // Only the original first entry's song artwork is eligible. Do not
            // scan later tracks, substitute a music-video thumbnail, or search.
            if let first = tracks.first, case .song(let song) = first {
                if let url = song.artwork?.url(width: 600, height: 600) {
                    album["firstTrackArtworkURL"] = url.absoluteString
                }
            }
            nextAlbums.append(album); completed += 1
        }
        for libraryAlbum in libraryAlbums {
            let tracks = try await retryEmptyAlbumRead(read: {
                let full = try await libraryAlbum.with(.tracks, preferredSource: .library)
                guard let collection = full.tracks else { throw failure("专辑曲目暂时不可用，已保留上次完整曲库") }
                return try await all(collection)
            }, reread: {
                var freshRequest = MusicLibraryRequest<Album>(); freshRequest.limit = 1
                freshRequest.filter(matching: \.id, equalTo: libraryAlbum.id)
                let response = try await freshRequest.response()
                guard let fresh = response.items.first, fresh.id == libraryAlbum.id else {
                    throw failure("专辑已变化，已保留上次完整曲库；请稍后重新同步")
                }
                let full = try await fresh.with(.tracks, preferredSource: .library)
                guard let collection = full.tracks else { throw failure("专辑曲目暂时不可用，已保留上次完整曲库") }
                return try await all(collection)
            })
            // The relationship is the user's library subset. Album.trackCount
            // and isComplete can describe the release, not the owned tracks.
            let metadata = tracks.map { AppleAlbumTrackIdentity(id: $0.id.rawValue, disc: $0.discNumber, number: $0.trackNumber) }
            let identity = appleAlbumIdentity(libraryAlbum.id.rawValue, metadata)
            var rows: [[String: Any]] = []
            for i in appleAlbumOrder(metadata) {
                let track = tracks[i], id = identity.occurrences[i]
                var row: [String: Any] = ["id": id, "albumId": identity.id, "source": "apple", "title": track.title,
                    "artist": track.artistName, "duration": track.duration ?? 0, "format": "Apple Music",
                    "browserPlayable": false, "audioUrl": "", "relativePath": "",
                    "sourcePosition": i, "snapshotRevision": identity.revision]
                if let number = track.trackNumber, number > 0 { row["trackNumber"] = number }
                if let disc = track.discNumber, disc > 0 { row["discNumber"] = disc }
                let supported: Bool
                if case .song = track { supported = true } else { supported = false }
                rows.append(row); nextItems[id] = Item(track: track, json: row, supported: supported)
            }
            var album: [String: Any] = ["id": identity.id, "source": "apple", "kind": "album",
                "title": libraryAlbum.title, "artist": libraryAlbum.artistName, "genreId": "apple-albums",
                "rawGenres": libraryAlbum.genreNames, "folder": "", "tracks": rows, "producers": [], "offline": false,
                "complete": !tracks.isEmpty, "snapshotRevision": identity.revision,
                "loadError": tracks.isEmpty ? "本次未读取到歌曲。若这本来是空专辑可忽略；若已收藏歌曲，请稍后重新同步。" as Any : NSNull()]
            if let year = appleReleaseYear(libraryAlbum.releaseDate) { album["year"] = year }
            if let url = libraryAlbum.artwork?.url(width: 600, height: 600) { album["artworkURL"] = url.absoluteString }
            if let first = tracks.first, case .song(let song) = first,
               let url = song.artwork?.url(width: 600, height: 600) { album["firstTrackArtworkURL"] = url.absoluteString }
            nextAlbums.append(album); completed += 1
        }
        // Publish after all reads finish; uncertain empty relationships remain marked.
        // An active queue keeps its own objects.
        items = nextItems; albums = nextAlbums
        return albums
    }
    func waitForOperations() async throws {
        let deadline = Date().addingTimeInterval(30)
        while inFlight > 0 {
            guard Date() < deadline else { throw failure("Apple Music 操作仍未结束，暂不能切换播放来源") }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
    }
    func settle() {
        if intent == "idle" { player.stop() }
        else if intent == "paused" { player.pause() }
    }
    func stop() async throws {
        generation += 1; let ticket = generation
        stopping += 1; defer { stopping -= 1 }
        intent = "idle"; player.stop()
        try await waitForOperations()
        guard ticket == generation else { throw failure("停止操作已被后续命令替代") }
        // Never trust a stale paused snapshot at a late play() completion.
        player.stop()
        let deadline = Date().addingTimeInterval(15)
        while player.state.playbackStatus == .playing {
            guard ticket == generation else { throw failure("停止操作已被后续命令替代") }
            guard Date() < deadline else { throw failure("Apple Music 尚未确认停止，已取消来源切换") }
            try await Task.sleep(nanoseconds: 10_000_000)
            player.stop()
        }
    }
    func control(_ operation: String, _ body: [String: Any]) async throws {
        if operation == "stop" { try await stop(); return }
        try checkAuthorization()
        generation += 1; let ticket = generation
        if operation == "toggle" {
            let playing = inFlight > 0 ? intent == "playing" : player.state.playbackStatus == .playing
            intent = playing ? "paused" : "playing"
        }
        else if operation == "play" { intent = "playing" }
        try await waitForOperations()
        guard ticket == generation else { throw failure("播放操作已被后续命令替代") }
        if operation != "play" && (runtimeIDs.isEmpty || player.queue.currentEntry.map { !runtimeIDs.contains($0.id) } ?? true) {
            intent = "idle"; runtimeIDs = []
            try await stopFailedOperation(ticket)
            throw failure("播放队列尚未核对，请重新点播歌曲")
        }
        if operation == "play" {
            guard let ids = body["ids"] as? [String], let selected = body["id"] as? String,
                  ids.filter({ $0 == selected }).count == 1, let index = ids.firstIndex(of: selected), !ids.isEmpty else {
                throw failure("无法唯一定位歌单条目")
            }
            if ids.contains(where: { items[$0] == nil }) { _ = try await sync() }
            guard ticket == generation else { throw failure("播放操作已取消") }
            let requested = try ids.map { id -> Item in
                guard let item = items[id] else { throw failure("资料库条目已变化，请重新同步后点播") }
                return item
            }
            let plan = try appleAudioQueuePlan(requested.map(\.supported), selected: index)
            let selectedNativeID = requested[index].track.id
            guard requested.filter({ $0.track.id == selectedNativeID }).count == 1 else {
                throw failure("此歌曲在当前队列重复出现，暂无法唯一定位所选位置")
            }
            originalIndices = plan.indices
            queue = plan.indices.map { requested[$0] }; runtimeIDs = []; lastIndex = plan.selected; queueGeneration += 1
            let tracks = queue.map(\.track)
            player.queue = ApplicationMusicPlayer.Queue(for: tracks, startingAt: tracks[lastIndex])
        }
        inFlight += 1
        defer { inFlight -= 1; settle() }
        do {
            switch operation {
            case "play":
                try await startVerifiedAppleQueue(prepare: { try await self.player.prepareToPlay() },
                    canStart: { ticket == self.generation && self.intent == "playing" },
                    verify: { try self.verifyQueue() }, play: { try await self.player.play() })
            case "toggle":
                if intent == "paused" { player.pause() } else if !queue.isEmpty { try await player.play() }
            case "next": try await player.skipToNextEntry()
            case "previous": try await player.skipToPreviousEntry()
            case "seek":
                guard let seconds = body["value"] as? Double, seconds.isFinite, seconds >= 0 else { throw failure("无效播放位置") }
                player.playbackTime = seconds
            default: throw failure("不支持的 Apple 播放操作")
            }
            if intent != "idle" && operation == "play" {
                try verifyQueue()
            }
            if ticket == generation && intent == "playing" && (operation == "play" || operation == "toggle") {
                let deadline = Date().addingTimeInterval(15)
                while player.state.playbackStatus != .playing && ticket == generation {
                    guard Date() < deadline else { throw failure("Apple Music 未能开始播放") }
                    try await Task.sleep(nanoseconds: 10_000_000)
                }
            }
        } catch {
            if ticket == generation {
                intent = "idle"; runtimeIDs = []
                try await stopFailedOperation(ticket)
            }
            throw error
        }
    }
    func playback() -> [String: Any] {
        let raw = player.state.playbackStatus
        if let current = player.queue.currentEntry, let index = runtimeIDs.firstIndex(of: current.id) { lastIndex = index }
        // Observe external media-key changes. Do not continuously enforce Pause.
        let mapped = player.queue.currentEntry.map { runtimeIDs.contains($0.id) } ?? false
        if inFlight == 0 && stopping == 0 && !queue.isEmpty && !mapped && raw == .playing {
            // An unverified or failed queue must never become valid via a later state poll.
            intent = "idle"; player.stop()
        }
        if inFlight == 0 && stopping == 0 && mapped {
            if raw == .playing { intent = "playing" }
            else if intent == "playing" && raw == .paused { intent = "paused" }
            else if raw == .stopped { intent = "idle" }
        }
        return ["playing": mapped && intent == "playing" && raw == .playing, "elapsed": intent == "idle" || !mapped ? 0 : player.playbackTime,
            "transport": intent, "currentIndex": originalIndices.indices.contains(lastIndex) ? originalIndices[lastIndex] : lastIndex, "queueGeneration": queueGeneration,
            "track": !runtimeIDs.isEmpty && queue.indices.contains(lastIndex) ? queue[lastIndex].json : NSNull(),
            "nativeState": String(describing: raw), "pendingOperations": inFlight]
    }
    func request(_ op: String, _ body: [String: Any]) async throws -> Any {
        switch op {
        case "status": return ["supported": true, "authorization": authorization(), "completed": completed, "total": total, "syncing": syncing]
        case "authorize":
            if MusicAuthorization.currentStatus == .notDetermined { _ = await MusicAuthorization.request() }
            return ["authorization": authorization()]
        case "sync": return try await sync()
        case "artwork": return try await artwork(body)
        case "state": return playback()
        default: try await control(op, body); return playback()
        }
    }
}

// Callback payload is valid only during the callback; Rust copies it immediately.
typealias Reply = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<CChar>?) -> Void
@_cdecl("rhine_apple_request")
func rhineAppleRequest(_ raw: UnsafePointer<CChar>, _ context: UnsafeMutableRawPointer?, _ callback: Reply) {
    let bytes = Data(String(cString: raw).utf8)
    Task { @MainActor in
        let response: [String: Any]
        do {
            let input = try JSONSerialization.jsonObject(with: bytes) as? [String: Any] ?? [:]
            let op = input["operation"] as? String ?? "status"
            if #available(macOS 14.0, *) {
                let value = try await AppleBridge.shared.request(op, input["body"] as? [String: Any] ?? [:])
                response = ["ok": true, "value": value]
            } else if op == "status" {
                response = ["ok": true, "value": ["supported": false, "authorization": "notDetermined"]]
            } else { throw failure("Apple Music 功能需要 macOS 14 或更新版本") }
        } catch { response = ["ok": false, "error": safeError(error)] }
        let data = (try? JSONSerialization.data(withJSONObject: response)) ?? Data("{\"ok\":false,\"error\":\"invalid native response\"}".utf8)
        String(decoding: data, as: UTF8.self).withCString { callback(context, $0) }
    }
}
