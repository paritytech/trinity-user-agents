import CryptoKit
import Foundation

protocol ChatResourceCaching: Sendable {
    func data(for key: String) async -> Data?
    func store(_ data: Data, for key: String) async
    func remove(for key: String) async
}

/// Keeps resource bytes in one directory and removes the least recently read files when the directory
/// grows past `maxBytes`.
///
/// The cache reads the directory once, at its first use, and then keeps the size and the read time of each file
/// in memory. A store therefore does not list the directory, and its cost does not grow with the number of
/// files (P7.1, B6).
actor ChatResourceDiskCache: ChatResourceCaching {
    private let directory: URL
    private let maxBytes: Int
    private let fileManager: FileManager
    private let now: @Sendable () -> Date

    /// The files of the directory, by name. Nil until the first use.
    private var files: [String: CachedFile]?
    private var totalBytes = 0

    init(
        directory: URL,
        maxBytes: Int,
        fileManager: FileManager = .default,
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.directory = directory
        self.maxBytes = maxBytes
        self.fileManager = fileManager
        self.now = now
    }

    func data(for key: String) -> Data? {
        let name = fileName(for: key)
        let file = directory.appending(path: name, directoryHint: .notDirectory)
        guard let data = try? Data(contentsOf: file) else { return nil }

        let readAt = now()
        try? fileManager.setAttributes([.modificationDate: readAt], ofItemAtPath: file.path)
        loadFiles()
        files?[name] = CachedFile(readAt: readAt, size: data.count)
        recount()
        return data
    }

    func store(_ data: Data, for key: String) {
        guard data.count <= maxBytes else { return }

        let name = fileName(for: key)
        do {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
            loadFiles()
            try data.write(to: directory.appending(path: name, directoryHint: .notDirectory), options: .atomic)
            files?[name] = CachedFile(readAt: now(), size: data.count)
            recount()
            trim()
        } catch {
            return
        }
    }

    func remove(for key: String) {
        let name = fileName(for: key)
        try? fileManager.removeItem(at: directory.appending(path: name, directoryHint: .notDirectory))
        files?[name] = nil
        recount()
    }
}

private struct CachedFile {
    let readAt: Date
    let size: Int
}

private extension ChatResourceDiskCache {
    func fileName(for key: String) -> String {
        SHA256.hash(data: Data(key.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    /// Reads the size and the read time of each file once. The directory can hold files of an earlier session.
    func loadFiles() {
        guard files == nil else { return }

        let keys: [URLResourceKey] = [.contentModificationDateKey, .fileSizeKey]
        let urls = (try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: keys)) ?? []
        var loaded: [String: CachedFile] = [:]
        for url in urls {
            guard let values = try? url.resourceValues(forKeys: Set(keys)) else { continue }
            loaded[url.lastPathComponent] = CachedFile(
                readAt: values.contentModificationDate ?? .distantPast,
                size: values.fileSize ?? 0
            )
        }
        files = loaded
        recount()
    }

    func recount() {
        totalBytes = files?.values.reduce(0) { $0 + $1.size } ?? 0
    }

    /// Removes the files that were read longest ago until the total size is within the bound.
    func trim() {
        guard totalBytes > maxBytes, let current = files else { return }

        for (name, file) in current.sorted(by: { $0.value.readAt < $1.value.readAt }) {
            guard totalBytes > maxBytes else { break }

            try? fileManager.removeItem(at: directory.appending(path: name, directoryHint: .notDirectory))
            files?[name] = nil
            totalBytes -= file.size
        }
    }
}
