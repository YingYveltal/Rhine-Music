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
        var albumRereads = 0
        let subset = try await retryEmptyAlbumRead(read: { [4, 7] }, reread: { albumRereads += 1; return [1, 2, 3, 4, 5, 6, 7] })
        precondition(subset == [4, 7] && albumRereads == 0, "Partial collections must not be expanded")
        let recoveredAlbum = try await retryEmptyAlbumRead(read: { [Int]() }, reread: { albumRereads += 1; return [7] })
        precondition(recoveredAlbum == [7] && albumRereads == 1)
        albumRereads = 0
        let emptyAlbum = try await retryEmptyAlbumRead(read: { [Int]() }, reread: { albumRereads += 1; return [Int]() })
        precondition(emptyAlbum.isEmpty && albumRereads == 1)
        albumRereads = 0
        do {
            _ = try await retryEmptyAlbumRead(read: { () async throws -> [Int] in throw ReadFailure.unavailable }, reread: { albumRereads += 1; return [1] })
            preconditionFailure("Failed album read must propagate")
        } catch ReadFailure.unavailable { precondition(albumRereads == 0) }
        do {
            _ = try await retryEmptyAlbumRead(read: { [Int]() }, reread: { () async throws -> [Int] in throw ReadFailure.incompletePage })
            preconditionFailure("Failed album re-read must propagate")
        } catch ReadFailure.incompletePage { }

        let meta = [AppleAlbumTrackIdentity(id: "a", disc: 2, number: 3),
                    AppleAlbumTrackIdentity(id: "same", disc: 1, number: 7),
                    AppleAlbumTrackIdentity(id: "same", disc: 1, number: 4),
                    AppleAlbumTrackIdentity(id: "tie", disc: 1, number: 7)]
        precondition(appleAlbumOrder(meta) == [2, 1, 3, 0], "Sort real disc/track numbers and keep tied occurrences")
        let missing = meta + [AppleAlbumTrackIdentity(id: "unknown", disc: nil, number: nil)]
        precondition(appleAlbumOrder(missing) == [0, 1, 2, 3, 4])
        let invalid = [AppleAlbumTrackIdentity(id: "zero", disc: 0, number: 0)] + meta
        precondition(appleAlbumOrder(invalid) == [0, 1, 2, 3, 4])
        let identity = appleAlbumIdentity("library-album", meta)
        precondition(identity.id.hasPrefix("apple-album-") && !identity.id.hasPrefix("apple-playlist-"))
        precondition(identity.occurrences.allSatisfy { $0.hasPrefix("apple-track-album-") })
        precondition(Set(identity.occurrences).count == meta.count)
        precondition(appleAlbumIdentity("library-album", meta).occurrences == identity.occurrences)
        precondition(appleAlbumIdentity("other-album", meta).occurrences != identity.occurrences)
        precondition(appleAlbumIdentity("library-album", Array(meta.reversed())).revision != identity.revision)
        precondition(appleReleaseYear(nil) == nil)
        precondition(appleReleaseYear(ISO8601DateFormatter().date(from: "2000-01-01T00:00:00Z")) == 2000)

        var pages = [(items: [3, 3], hasNext: true), (items: [5], hasNext: false)]
        let paged = try await readApplePages(initial: [1, 2], hasNext: true, next: { pages.removeFirst() })
        precondition(paged == [1, 2, 3, 3, 5] && pages.isEmpty)
        let single = try await readApplePages(initial: [1], hasNext: false, next: { preconditionFailure("Do not request nonexistent pages") })
        precondition(single == [1])
        for missingPage in [true, false] {
            do {
                _ = try await readApplePages(initial: [1], hasNext: true, next: { missingPage ? nil : ([], false) })
                preconditionFailure("A missing promised page cannot publish a partial library")
            } catch { precondition((error as NSError).domain == "RhineApple") }
        }
        do {
            _ = try await readApplePages(initial: [1], hasNext: true, next: { throw ReadFailure.incompletePage })
            preconditionFailure("Pagination errors must propagate")
        } catch ReadFailure.incompletePage { }
        print("Apple library album policies passed: subset, empty/retry failures, numbering, identity, year, pagination. No account/network access.")
        print("Apple playlist refresh: 6 policy checks passed (no account or network access).")
    }
}
