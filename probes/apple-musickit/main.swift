import Cocoa
import MusicKit

// A bounded native MusicKit experiment. No catalog, subscription preflight,
// developer-token provider, Apple Events or accessibility automation.
@MainActor final class Probe: NSObject, NSApplicationDelegate {
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
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 980, height: 660), styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Rhine Apple Music Probe · MusicKit"
        label("MusicKit 原生本机验证", NSRect(x: 20, y: 605, width: 940, height: 30)).font = .boldSystemFont(ofSize: 23)
        label("先授权和读取有限样本；播放前确保 Music 与 QQ 已停止。本模式维护独立队列，不遥控 Music.app。", NSRect(x: 20, y: 562, width: 940, height: 36))
        button("查看授权状态", #selector(authStatus), NSRect(x: 20, y: 508, width: 150, height: 32))
        button("请求音乐资料库授权", #selector(authorize), NSRect(x: 180, y: 508, width: 200, height: 32))
        button("读取已下载（最多3首）", #selector(readDownloaded), NSRect(x: 395, y: 508, width: 255, height: 32))
        button("读取现有库（最多3首）", #selector(readLibrary), NSRect(x: 665, y: 508, width: 280, height: 32))
        status = label("尚未请求授权", NSRect(x: 20, y: 450, width: 940, height: 46))
        samples = label("尚未读取样本。下载筛选不等于证明无 DRM；未核对来源的样本会保持未知。", NSRect(x: 20, y: 270, width: 940, height: 175))
        button("样本队列从第2首开始", #selector(playMiddle), NSRect(x: 20, y: 215, width: 230, height: 34))
        button("播放首个样本", #selector(playFirst), NSRect(x: 260, y: 215, width: 150, height: 34))
        button("暂停", #selector(pause), NSRect(x: 420, y: 215, width: 100, height: 34), lockedWhileBusy: false)
        button("继续", #selector(resume), NSRect(x: 530, y: 215, width: 100, height: 34))
        button("停止", #selector(stop), NSRect(x: 640, y: 215, width: 100, height: 34), lockedWhileBusy: false)
        button("下一首", #selector(next), NSRect(x: 750, y: 215, width: 100, height: 34))
        seek = NSTextField(frame: NSRect(x: 20, y: 166, width: 125, height: 28)); seek.stringValue = "60"; seek.setAccessibilityLabel("定位秒数"); window.contentView!.addSubview(seek)
        button("定位秒数", #selector(seekTo), NSRect(x: 160, y: 164, width: 125, height: 32))
        now = label("播放器尚未创建；读取曲库成功不等于可播放订阅歌曲。", NSRect(x: 20, y: 25, width: 940, height: 120))
        let menu = NSMenu(), item = NSMenuItem(), sub = NSMenu(); sub.addItem(withTitle: "退出探针", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"); item.submenu = sub; menu.addItem(item); NSApp.mainMenu = menu
        window.center(); window.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
        log("launch", ["bundleID": Bundle.main.bundleIdentifier ?? "", "os": ProcessInfo.processInfo.operatingSystemVersionString, "mode": "nativeMusicKit", "authorization": MusicAuthorization.currentStatus.rawValue])
        authStatus()
        Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in Task { @MainActor in self?.poll() } }
    }
    func run(_ event: String, work: @escaping @MainActor () async throws -> Void) {
        guard !busy else { return }; busy = true; controls.forEach { $0.isEnabled = false }; status.stringValue = event + "…"
        Task { @MainActor in
            defer { busy = false; controls.forEach { $0.isEnabled = true } }
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
    func play(index: Int) {
        guard songs.indices.contains(index) else { status.stringValue = "该位置没有样本；不得以单首冒充中间队列测试。"; return }
        let captured = songs, scope = sampleScope; generation += 1; let ticket = generation
        run("原生队列播放") {
            let p = ApplicationMusicPlayer.shared; self.player = p
            p.queue = ApplicationMusicPlayer.Queue(for: captured, startingAt: captured[index])
            self.log("queueRequest", ["scope": scope, "ids": captured.map { $0.id.rawValue }, "startIndex": index, "startID": captured[index].id.rawValue])
            try await p.play()
            if ticket != self.generation { p.stop(); self.log("cancelledLatePlayback"); return }
            self.poll()
        }
    }
    @objc func playMiddle() { play(index: 1) }
    @objc func playFirst() { play(index: 0) }
    @objc func pause() { generation += 1; player?.pause(); log("pause"); poll() }
    @objc func stop() { generation += 1; player?.stop(); log("stop"); poll() }
    @objc func resume() {
        guard let p = player else { return }; generation += 1; let ticket = generation
        run("继续播放") { try await p.play(); if ticket != self.generation { p.stop(); self.log("cancelledLatePlayback") } }
    }
    @objc func next() { guard let p = player else { return }; run("下一首") { try await p.skipToNextEntry() } }
    @objc func seekTo() {
        guard let p = player, let time = Double(seek.stringValue), time.isFinite, time >= 0 else { return }
        p.playbackTime = time; log("seek", ["seconds": time]); poll()
    }
    func poll() {
        guard let p = player else { return }
        let entry = p.queue.currentEntry, state = String(describing: p.state.playbackStatus), time = p.playbackTime
        let id = entry?.item?.id.rawValue ?? "", title = entry?.title ?? ""
        log("player", ["state": state, "position": time.isFinite ? time : 0, "entryID": entry?.id ?? "", "songID": id, "title": title, "prepared": p.isPreparedToPlay])
        now.stringValue = "\(state) · \(String(format: "%.2f", time)) 秒\n\(title)\n条目 \(entry?.id ?? "—") / 歌曲 \(id)"
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
