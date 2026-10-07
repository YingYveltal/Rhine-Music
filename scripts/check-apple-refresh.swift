import Foundation

// Exercise the same bounded-read policy used by the MusicKit bridge, without
// authorizing an account, fetching user content, or creating a playlist.
@main enum ApplePlaylistRefreshChecks {
    enum ReadFailure: Error { case unavailable, incompletePage }
    static func main() async throws {
        var reads = 0, rereads = 0
        let populated = try await retryEmptyPlaylistRead(read: {
            reads += 1; return (entries: [1, 2], tracks: [10, 20])
        }, reread: {
            rereads += 1; return (entries: [Int](), tracks: [Int]())
        })
        precondition(reads == 1 && rereads == 0 && populated.entries == [1, 2])

        reads = 0; rereads = 0
        let recovered = try await retryEmptyPlaylistRead(read: {
            reads += 1; return (entries: [Int](), tracks: [Int]())
        }, reread: {
            rereads += 1; return (entries: [3, 4, 5], tracks: [30, 40, 30])
        })
        precondition(reads == 1 && rereads == 1)
        precondition(recovered.entries == [3, 4, 5] && recovered.tracks == [30, 40, 30])

        reads = 0; rereads = 0
        let empty = try await retryEmptyPlaylistRead(read: {
            reads += 1; return (entries: [Int](), tracks: [Int]())
        }, reread: {
            rereads += 1; return (entries: [Int](), tracks: [Int]())
        })
        precondition(reads == 1 && rereads == 1 && empty.entries.isEmpty && empty.tracks.isEmpty)

        // One missing side is still an inconsistent snapshot for the caller's
        // order/count validation, not a reason to retry until it looks complete.
        rereads = 0
        let partial = try await retryEmptyPlaylistRead(read: {
            (entries: [1], tracks: [Int]())
        }, reread: {
            rereads += 1; return (entries: [1], tracks: [10])
        })
        precondition(rereads == 0 && partial.entries.count != partial.tracks.count)

        rereads = 0
        do {
            _ = try await retryEmptyPlaylistRead(read: { () async throws -> (entries: [Int], tracks: [Int]) in
                throw ReadFailure.incompletePage
            }, reread: {
                rereads += 1; return (entries: [1], tracks: [10])
            })
            preconditionFailure("A failed first read must propagate")
        } catch ReadFailure.incompletePage { precondition(rereads == 0) }

        rereads = 0
        do {
            _ = try await retryEmptyPlaylistRead(read: {
                (entries: [Int](), tracks: [Int]())
            }, reread: { () async throws -> (entries: [Int], tracks: [Int]) in
                rereads += 1; throw ReadFailure.unavailable
            })
            preconditionFailure("A failed re-read must propagate")
        } catch ReadFailure.unavailable { precondition(rereads == 1) }
        print("Apple playlist refresh: 6 policy checks passed (no account or network access).")
    }
}
