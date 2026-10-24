import Foundation
import os
@testable import Products

final class InMemoryDotNsContentStorage: DotNsContentStorageProtocol, @unchecked Sendable {
    private let saved = OSAllocatedUnfairLock(initialState: [String: Date]())
    private let savedAt: Date

    init(savedAt: Date = Date(timeIntervalSince1970: 0)) {
        self.savedAt = savedAt
    }

    var savedHashes: Set<String> { saved.withLock { Set($0.keys) } }

    func directory(for contentHash: String) -> URL {
        URL(fileURLWithPath: "/content").appendingPathComponent(contentHash)
    }

    func saveContent(contentHash: String, write _: (CarContentWriter) throws -> Void) throws {
        saved.withLock { $0[contentHash] = savedAt }
    }

    func getContentDirectory(contentHash: String) -> URL? {
        contentExists(contentHash: contentHash) ? directory(for: contentHash) : nil
    }

    func contentExists(contentHash: String) -> Bool {
        saved.withLock { $0[contentHash] != nil }
    }

    func deleteContent(contentHash: String) throws {
        saved.withLock { _ = $0.removeValue(forKey: contentHash) }
    }

    func loadContent(contentHash: String, relativePath _: String) -> Data? {
        contentExists(contentHash: contentHash) ? Data(contentHash.utf8) : nil
    }

    func deleteAll() throws {
        saved.withLock { $0.removeAll() }
    }

    func storedContents() -> [DotNsStoredContent] {
        saved.withLock { $0.map { DotNsStoredContent(contentHash: $0.key, modifiedAt: $0.value) } }
    }
}
