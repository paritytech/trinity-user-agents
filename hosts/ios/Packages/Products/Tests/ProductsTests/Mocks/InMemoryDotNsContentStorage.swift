import Foundation
import os
@testable import Products

final class InMemoryDotNsContentStorage: DotNsContentStorageProtocol, @unchecked Sendable {
    private let saved = OSAllocatedUnfairLock(initialState: Set<String>())

    var savedHashes: Set<String> { saved.withLock { $0 } }

    func directory(for contentHash: String) -> URL {
        URL(fileURLWithPath: "/content").appendingPathComponent(contentHash)
    }

    func saveContent(contentHash: String, write _: (CarContentWriter) throws -> Void) throws {
        saved.withLock { _ = $0.insert(contentHash) }
    }

    func getContentDirectory(contentHash: String) -> URL? {
        contentExists(contentHash: contentHash) ? directory(for: contentHash) : nil
    }

    func contentExists(contentHash: String) -> Bool {
        saved.withLock { $0.contains(contentHash) }
    }

    func deleteContent(contentHash: String) throws {
        saved.withLock { _ = $0.remove(contentHash) }
    }

    func loadContent(contentHash: String, relativePath _: String) -> Data? {
        contentExists(contentHash: contentHash) ? Data(contentHash.utf8) : nil
    }

    func deleteAll() throws {
        saved.withLock { $0.removeAll() }
    }
}
