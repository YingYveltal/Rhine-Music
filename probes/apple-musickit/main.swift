import Cocoa
import MusicKit

// A bounded native MusicKit experiment. No catalog, subscription preflight,
// developer-token provider, Apple Events or accessibility automation.
@MainActor final class Probe: NSObject, NSApplicationDelegate, NSTableViewDataSource {
    var window: NSWindow!
    var status: NSTextField!
    var samples: NSTextField!
    var now: NSTextField!
    var seek: NSTextField!
    var controls: [NSButton] = []
    var songs: [Song] = []
    var sampleScope = "none"
    var downloadedIDs = Set<MusicItemID>()
    var player: ApplicationMusicPlayer?
    var busy = false
    var generation = 0
    var queueGeneration = 0
    var intent = "stopped"
    var operation: Task<Void, Never>?
    var playlistMenu: NSPopUpButton!
    var playlistTable: NSTableView!
    var playlistScroll: NSScrollView!
    var playlists: [Playlist] = []
    var playlistEntries: [Playlist.Entry] = []
    var playlistTracks: [Track] = []
    var loadedPlaylist: Playlist?
    var queueSources: [[String: Any]] = []
    var lastMapping: Data?
    var logURL: URL!

    func log(_ event: String, _ data: [String: Any] = [:]) {
        let row: [String: Any] = ["time": ISO8601DateFormatter().string(from: Date()), "event": event, "data": data]
        guard var bytes = try? JSONSerialization.data(withJSONObject: row) else { return }
        bytes.append(10)
        if !FileManager.default.fileExists(atPath: logURL.path) { FileManager.default.createFile(atPath: logURL.path, contents: nil) }
        guard let handle = try? FileHandle(forWritingTo: logURL) else { return }
        defer { try? handle.close() }
        do { try handle.seekToEnd(); try handle.write(contentsOf: bytes) } catch { }
    }
    // Only error domains/codes, not userInfo, request URLs or token-bearing descriptions.
    func errorCodes(_ error: Error) -> [[String: Any]] {
        var chain: [[String: Any]] = [], current: NSError? = error as NSError
        for _ in 0..<5 {
            guard let value = current else { break }
            chain.append(["domain": value.domain, "code": value.code])
            current = value.userInfo[NSUnderlyingErrorKey] as? NSError
        }
        return chain
    }
    @discardableResult func label(_ text: String, _ rect: NSRect) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: text); field.frame = rect
        window.contentView!.addSubview(field); return field
    }
    func button(_ text: String, _ action: Selector, _ rect: NSRect, lockedWhileBusy: Bool = true) {
        let b = NSButton(title: text, target: self, action: action); b.frame = rect
        window.contentView!.addSubview(b); if lockedWhileBusy { controls.append(b) }
    }
    func applicationDidFinishLaunching(_ notification: Notification) {
        let directory = Bundle.main.object(forInfoDictionaryKey: "RhineProbeEvidenceDirectory") as! String
        try? FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
        logURL = URL(fileURLWithPath: directory).appendingPathComponent("events.jsonl")
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1040, height: 820), styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Rhine Apple Music Probe · MusicKit"
        label("MusicKit 原生本机验证", NSRect(x: 20, y: 765, width: 940, height: 30)).font = .boldSystemFont(ofSize: 23)
        label("先授权和读取有限样本；播放前确保 Music 与 QQ 已停止。本模式维护独立队列，不遥控 Music.app。", NSRect(x: 20, y: 722, width: 940, height: 36))
        button("查看授权状态", #selector(authStatus), NSRect(x: 20, y: 668, width: 150, height: 32))
        button("请求音乐资料库授权", #selector(authorize), NSRect(x: 180, y: 668, width: 200, height: 32))
        button("读取已下载（最多3首）", #selector(readDownloaded), NSRect(x: 395, y: 668, width: 255, height: 32))
        button("读取现有库（最多3首）", #selector(readLibrary), NSRect(x: 665, y: 668, width: 280, height: 32))
        button("读取已有歌单（最多10个）", #selector(readPlaylists), NSRect(x: 20, y: 565, width: 245, height: 32))
        playlistMenu = NSPopUpButton(frame: NSRect(x: 275, y: 565, width: 335, height: 32)); window.contentView!.addSubview(playlistMenu)
        button("读取所选歌单顺序", #selector(readPlaylistEntries), NSRect(x: 620, y: 565, width: 190, height: 32))
        button("从所选歌单行播放", #selector(playPlaylistRow), NSRect(x: 820, y: 565, width: 200, height: 32))
        playlistTable = NSTableView(frame: .zero); playlistTable.dataSource = self; playlistTable.rowHeight = 25
        for (index, pair) in [("顺序",60.0),("曲目",440.0),("艺人",290.0),("时长",160.0)].enumerated() {
            let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier(String(index))); column.title = pair.0; column.width = pair.1; playlistTable.addTableColumn(column)
        }
        playlistScroll = NSScrollView(frame: NSRect(x: 20, y: 315, width: 1000, height: 240)); playlistScroll.documentView = playlistTable; playlistScroll.hasVerticalScroller = true; playlistScroll.isHidden = true; window.contentView!.addSubview(playlistScroll)
        button("上一首", #selector(previous), NSRect(x: 860, y: 260, width: 100, height: 34))
        status = label("尚未请求授权", NSRect(x: 20, y: 610, width: 940, height: 46))
        samples = label("尚未读取样本。下载筛选不等于证明无 DRM；未核对来源的样本会保持未知。", NSRect(x: 20, y: 315, width: 940, height: 240))
        button("样本队列从第2首开始", #selector(playMiddle), NSRect(x: 20, y: 260, width: 230, height: 34))
        button("播放首个样本", #selector(playFirst), NSRect(x: 260, y: 260, width: 150, height: 34))
        button("暂停", #selector(pause), NSRect(x: 420, y: 260, width: 100, height: 34), lockedWhileBusy: false)
        button("继续", #selector(resume), NSRect(x: 530, y: 260, width: 100, height: 34))
        button("停止", #selector(stop), NSRect(x: 640, y: 260, width: 100, height: 34), lockedWhileBusy: false)
        button("下一首", #selector(next), NSRect(x: 750, y: 260, width: 100, height: 34))
        seek = NSTextField(frame: NSRect(x: 20, y: 211, width: 125, height: 28)); seek.stringValue = "60"; seek.setAccessibilityLabel("定位秒数"); window.contentView!.addSubview(seek)
        button("定位秒数", #selector(seekTo), NSRect(x: 160, y: 209, width: 125, height: 32))
        now = label("播放器尚未创建；读取曲库成功不等于可播放订阅歌曲。", NSRect(x: 20, y: 40, width: 1000, height: 150))
        let menu = NSMenu(), item = NSMenuItem(), sub = NSMenu(); sub.addItem(withTitle: "退出探针", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"); item.submenu = sub; menu.addItem(item); NSApp.mainMenu = menu
        window.center(); window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
        log("launch", ["bundleID": Bundle.main.bundleIdentifier ?? "", "os": ProcessInfo.processInfo.operatingSystemVersionString, "mode": "nativeMusicKit", "authorization": MusicAuthorization.currentStatus.rawValue])
        authStatus()
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in Task { @MainActor in self?.poll() } }
    }
    func run(_ event: String, work: @escaping @MainActor () async throws -> Void) {
        guard !busy else { return }; busy = true; controls.forEach { $0.isEnabled = false }; status.stringValue = event + "…"
        operation = Task { @MainActor in
            defer { busy = false; operation = nil; controls.forEach { $0.isEnabled = true } }
            log(event + ".begin")
            do { try await work(); log(event + ".end", ["ok": true]); status.stringValue = event + "完成" }
            catch { let codes = errorCodes(error); log(event + ".end", ["ok": false, "errors": codes]); status.stringValue = event + "失败：" + codes.map { "\($0["domain"] ?? "") / \($0["code"] ?? "")" }.joined(separator: " → ") }
        }
    }
    @objc func authStatus() { let value = MusicAuthorization.currentStatus.rawValue; log("authorizationStatus", ["status": value]); status.stringValue = "音乐资料库授权：" + value }
    @objc func authorize() {
        run("请求授权") {
            let before = MusicAuthorization.currentStatus
            self.log("authorizationBefore", ["status": before.rawValue])
            let after = before == .notDetermined ? await MusicAuthorization.request() : before
            self.log("authorizationAfter", ["status": after.rawValue])
            self.samples.stringValue = "授权结果：" + after.rawValue + "。随后分别执行两个只读请求。"
        }
    }
    func read(_ downloaded: Bool) {
        guard MusicAuthorization.currentStatus == .authorized else { authStatus(); return }
        run(downloaded ? "读取已下载样本" : "读取现有库样本") {
            self.playlistScroll.isHidden = true; self.samples.isHidden = false
            self.songs = []; self.samples.stringValue = "正在读取…"
            var request = MusicLibraryRequest<Song>(); request.limit = 3; request.includeOnlyDownloadedContent = downloaded
            self.log("libraryRequest", ["downloadedOnly": downloaded, "limit": 3])
            let response = try await request.response()
            self.songs = Array(response.items.prefix(3)); self.sampleScope = downloaded ? "downloadedOnly" : "library"
            if downloaded { self.downloadedIDs = Set(self.songs.map(\.id)) }
            let rows: [[String: Any]] = self.songs.enumerated().map { index, song in
                ["index": index, "id": song.id.rawValue, "title": song.title, "artist": song.artistName, "album": (song.albumTitle ?? "") as String, "duration": song.duration ?? 0, "hasPlayParameters": song.playParameters != nil, "observedDownloaded": downloaded || self.downloadedIDs.contains(song.id), "protectionOrSubscription": "notClassified"]
            }
            self.log("libraryResponse", ["scope": self.sampleScope, "count": rows.count, "rows": rows])
            self.samples.stringValue = "\(self.sampleScope)：\(self.songs.count) 首（来源/保护类型待核对）\n" + self.songs.enumerated().map { "\($0.offset + 1). \($0.element.title) · \($0.element.artistName) · \($0.element.duration ?? 0) 秒" }.joined(separator: "\n")
        }
    }
    @objc func readDownloaded() { read(true) }
    @objc func readLibrary() { read(false) }
    @objc func readPlaylists() {
        guard MusicAuthorization.currentStatus == .authorized else { authStatus(); return }
        run("读取已有歌单") {
            var request = MusicLibraryRequest<Playlist>(); request.limit = 10
            self.log("playlistsRequest", ["limit": 10])
            let result = try await request.response()
            self.playlists = Array(result.items.prefix(10))
            self.playlistMenu.removeAllItems(); self.playlistMenu.addItems(withTitles: self.playlists.map(\.name))
            self.log("playlistsResponse", ["hasNextBatch": result.items.hasNextBatch, "rows": self.playlists.map { ["id": $0.id.rawValue, "name": $0.name] }])
        }
    }
    @objc func readPlaylistEntries() {
        let index = playlistMenu.indexOfSelectedItem
        guard playlists.indices.contains(index) else { return }
        let playlist = playlists[index]
        run("读取歌单条目") {
            self.playlistEntries = []; self.playlistTracks = []; self.loadedPlaylist = nil; self.playlistTable.reloadData()
            self.log("playlistRelationshipRequest", ["playlistID": playlist.id.rawValue, "relationship": "entries", "preferredSource": "library"])
            let hydrated = try await playlist.with(.entries, preferredSource: .library)
            guard var batch = hydrated.entries else { throw NSError(domain: "RhineProbe.MissingPlaylistEntries", code: 1) }
            var entries = Array(batch.prefix(250)); var pages = 1
            while batch.hasNextBatch && entries.count < 250 {
                guard let next = try await batch.nextBatch(limit: 250 - entries.count), !next.isEmpty else { break }
                entries.append(contentsOf: next.prefix(250 - entries.count)); batch = next; pages += 1
            }
            self.log("playlistRelationshipRequest", ["playlistID": playlist.id.rawValue, "relationship": "tracks", "preferredSource": "library"])
            let tracksPlaylist = try await playlist.with(.tracks, preferredSource: .library)
            guard var tracksBatch = tracksPlaylist.tracks else { throw NSError(domain: "RhineProbe.MissingPlaylistTracks", code: 2) }
            var tracks = Array(tracksBatch.prefix(250)); var trackPages = 1
            while tracksBatch.hasNextBatch && tracks.count < 250 {
                guard let next = try await tracksBatch.nextBatch(limit: 250 - tracks.count), !next.isEmpty else { break }
                tracks.append(contentsOf: next.prefix(250 - tracks.count)); tracksBatch = next; trackPages += 1
            }
            let sameOrder = tracks.count == entries.count && zip(tracks, entries).allSatisfy { $0.0.title == $0.1.title && $0.0.artistName == $0.1.artistName }
            self.log("playlistTracksResponse", ["count": tracks.count, "pages": trackPages, "hasNextBatch": tracksBatch.hasNextBatch, "sameOrderAsEntries": sameOrder, "rows": tracks.enumerated().map { ["ordinal": $0.offset, "libraryTrackID": $0.element.id.rawValue, "title": $0.element.title, "artist": $0.element.artistName, "hasPlayParameters": $0.element.playParameters != nil] }])
            guard sameOrder else { throw NSError(domain: "RhineProbe.PlaylistOrderMismatch", code: 3) }
            self.playlistEntries = entries; self.playlistTracks = tracks; self.loadedPlaylist = hydrated
            self.samples.isHidden = true; self.playlistScroll.isHidden = false; self.playlistTable.reloadData()
            if !entries.isEmpty { self.playlistTable.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false) }
            self.log("playlistRelationshipResponse", ["playlistID": hydrated.id.rawValue, "count": entries.count, "pages": pages, "hasNextBatch": batch.hasNextBatch, "rows": entries.enumerated().map { self.playlistSource($0.element, index: $0.offset) }])
        }
    }
    func playlistSource(_ entry: Playlist.Entry, index: Int) -> [String: Any] {
        ["ordinal": index, "playlistEntryID": entry.id.rawValue, "libraryItemID": entry.item?.id.rawValue ?? "", "position": entry.position, "title": entry.title, "artist": entry.artistName, "duration": entry.duration ?? 0, "itemType": entry.item.map { item in switch item { case .song: return "song"; case .musicVideo: return "musicVideo"; @unknown default: return "unknown" } } ?? "nil", "hasEntryPlayParameters": entry.playParameters != nil, "hasItemPlayParameters": entry.item?.playParameters != nil]
    }
    func numberOfRows(in tableView: NSTableView) -> Int { playlistEntries.count }
    func tableView(_ tableView: NSTableView, objectValueFor tableColumn: NSTableColumn?, row: Int) -> Any? {
        let entry = playlistEntries[row]
        switch tableColumn?.identifier.rawValue {
        case "0": return row + 1
        case "1": return entry.title
        case "2": return entry.artistName
        default: return String(format: "%.2f", entry.duration ?? 0)
        }
    }
    @objc func playPlaylistRow() {
        let index = playlistTable.selectedRow
        guard playlistEntries.indices.contains(index), let playlist = loadedPlaylist else { return }
        let entries = playlistEntries, tracks = playlistTracks
        guard tracks.count == entries.count else { return }
        let sources: [[String: Any]] = entries.enumerated().map { index, entry in
            var source = playlistSource(entry, index: index); source["libraryTrackID"] = tracks[index].id.rawValue; return source
        }
        startQueue(tracks.map { MusicPlayer.Queue.Entry($0) }, sources: sources, index: index, scope: "playlist:" + playlist.id.rawValue, tracks: tracks)
    }
    func play(index: Int) {
        guard songs.indices.contains(index) else { status.stringValue = "该位置没有样本；不得以单首冒充中间队列测试。"; return }
        let captured = songs
        startQueue(captured.map { MusicPlayer.Queue.Entry($0) }, sources: captured.enumerated().map { ["ordinal": $0.offset, "libraryItemID": $0.element.id.rawValue, "title": $0.element.title] }, index: index, scope: sampleScope, sampleSongs: captured)
    }
    func startQueue(_ entries: [MusicPlayer.Queue.Entry], sources: [[String: Any]], index: Int, scope: String, tracks: [Track]? = nil, sampleSongs: [Song]? = nil) {
        guard !busy else { return }
        generation += 1; let ticket = generation; queueGeneration += 1; intent = "playing"
        queueSources = zip(sources, entries).map { source, entry in var row = source; row["inputQueueEntryID"] = entry.id; return row }
        lastMapping = nil
        run("原生队列播放") {
            let p = ApplicationMusicPlayer.shared; self.player = p
            if let tracks {
                p.queue = ApplicationMusicPlayer.Queue(for: tracks, startingAt: tracks[index])
            } else if let sampleSongs {
                p.queue = ApplicationMusicPlayer.Queue(for: sampleSongs, startingAt: sampleSongs[index])
            } else { throw NSError(domain: "RhineProbe.MissingQueueSource", code: 4) }
            self.log("queueRequest", ["scope": scope, "constructor": tracks == nil ? "librarySongs" : "libraryTracks", "queueGeneration": self.queueGeneration, "commandGeneration": ticket, "sources": self.queueSources, "startIndex": index])
            self.recordMapping()
            do { try await p.play() }
            catch { if ticket == self.generation { self.intent = "stopped" }; self.settle(ticket); throw error }
            self.settle(ticket); self.recordMapping(); self.poll()
        }
    }
    // Every async playback command returns through the same latest-intent barrier.
    // A late SDK completion can still have transient effects; logs/tests bound that risk.
    func settle(_ ticket: Int) {
        if ticket != generation { log("staleCommandCompletion", ["ticket": ticket, "latest": generation, "intent": intent]) }
        guard let p = player else { return }
        // SDK status may still say paused when play() completes, before a late
        // transition to playing. Always send the latest terminal intent here.
        if intent == "stopped" || intent == "paused" {
            log("completionIntent", ["ticket": ticket, "latest": generation, "intent": intent, "nativeStateBefore": String(describing: p.state.playbackStatus)])
            if intent == "stopped" { p.stop() } else { p.pause() }
        }
    }
    func enforceIntent() {
        guard let p = player else { return }
        if intent == "stopped" {
            if p.state.playbackStatus == .playing { p.stop(); log("reassertStop", ["commandGeneration": generation]) }
        } else if intent == "paused" && p.state.playbackStatus != .paused { p.pause() }
    }
    @objc func playMiddle() { play(index: 1) }
    @objc func playFirst() { play(index: 0) }
    @objc func pause() {
        generation += 1; intent = "paused"; operation?.cancel(); player?.pause()
        log("pause", ["commandGeneration": generation, "queueGeneration": queueGeneration]); poll()
    }
    @objc func stop() {
        generation += 1; intent = "stopped"; operation?.cancel(); player?.stop(); enforceIntent()
        log("stop", ["commandGeneration": generation, "queueGeneration": queueGeneration]); poll()
    }
    @objc func resume() {
        guard !busy, let p = player, !p.queue.entries.isEmpty, intent != "stopped" else { return }
        generation += 1; intent = "playing"; let ticket = generation
        run("继续播放") {
            do { try await p.play() } catch { if ticket == self.generation { self.intent = "stopped" }; self.settle(ticket); throw error }
            self.settle(ticket)
        }
    }
    func skip(_ forward: Bool) {
        guard !busy, let p = player, !p.queue.entries.isEmpty, intent != "stopped" else { return }
        generation += 1; let ticket = generation
        run(forward ? "下一首" : "上一首") {
            self.log("skipRequest", ["forward": forward, "commandGeneration": ticket, "queueGeneration": self.queueGeneration])
            do { if forward { try await p.skipToNextEntry() } else { try await p.skipToPreviousEntry() } }
            catch { self.settle(ticket); throw error }
            self.settle(ticket)
        }
    }
    @objc func next() { skip(true) }
    @objc func previous() { skip(false) }
    @objc func seekTo() {
        guard !busy, let p = player, intent != "stopped", let time = Double(seek.stringValue), time.isFinite, time >= 0 else { return }
        generation += 1; p.playbackTime = time; log("seek", ["seconds": time, "commandGeneration": generation]); poll()
    }
    func recordMapping() {
        guard let p = player else { return }
        let observed = Array(p.queue.entries)
        let aligned = observed.count == queueSources.count && zip(observed, queueSources).allSatisfy { $0.0.title == ($0.1["title"] as? String) }
        let rows: [[String: Any]] = observed.enumerated().map { index, entry in
            var row: [String: Any] = ["runtimeOrdinal": index, "entryID": entry.id, "resolvedItemID": entry.item?.id.rawValue ?? "", "observedTitle": entry.title]
            if aligned && queueSources.indices.contains(index) { row["constructorSource"] = queueSources[index] }
            return row
        }
        let mapping: [String: Any] = ["queueGeneration": queueGeneration, "commandGeneration": generation, "sourceCount": queueSources.count, "observedCount": rows.count, "fullOrderedTitlesAligned": aligned, "rows": rows]
        let data = try? JSONSerialization.data(withJSONObject: mapping, options: [.sortedKeys])
        if data != lastMapping { log("queueMapping", mapping); lastMapping = data }
    }
    func poll() {
        guard let p = player else { return }
        // No EOF inference or play command is issued here. Only explicit Pause/Stop
        // intent is re-applied if an in-flight SDK operation finishes late.
        if intent != "playing" {
            log("playerBeforeIntent", ["intent": intent, "nativeState": String(describing: p.state.playbackStatus), "position": p.playbackTime.isFinite ? p.playbackTime : 0, "entryID": p.queue.currentEntry?.id ?? "", "commandGeneration": generation, "queueGeneration": queueGeneration])
            enforceIntent()
        }
        recordMapping()
        let entry = p.queue.currentEntry, state = String(describing: p.state.playbackStatus), time = p.playbackTime
        let id = entry?.item?.id.rawValue ?? "", title = entry?.title ?? ""
        log("player", ["state": state, "position": time.isFinite ? time : 0, "entryID": entry?.id ?? "", "songID": id, "title": title, "prepared": p.isPreparedToPlay, "queueGeneration": queueGeneration, "commandGeneration": generation, "intent": intent, "shuffle": p.state.shuffleMode.map { String(describing: $0) } ?? "unknown", "repeat": p.state.repeatMode.map { String(describing: $0) } ?? "unknown"])
        now.stringValue = "意图 \(intent) / 原生 \(state) · \(String(format: "%.2f", time)) 秒\n\(title)\n条目 \(entry?.id ?? "—") / 歌曲 \(id)"
    }
    func applicationWillTerminate(_ notification: Notification) { player?.stop(); log("quit") }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}
@main struct EntryPoint {
    @MainActor static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.regular)
        let delegate = Probe()
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
}
