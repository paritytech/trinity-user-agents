import CryptoKit
import Foundation

protocol ChatResourceCaching: Sendable {
    func data(for key: String) async -> Data?
    func store(_ data: Data, for key: String) async
}

/// Keeps resource bytes in one directory and removes the least recently read files when the directory
/// grows past `maxBytes`.
actor ChatResourceDiskCache: ChatResourceCaching {
    private let directory: URL
    private let maxBytes: Int
    private let fileManager: FileManager

    init(directory: URL, maxBytes: Int, fileManager: FileManager = .default) {
        self.directory = directory
        self.maxBytes = maxBytes
        self.fileManager = fileManager
    }

    func data(for key: String) -> Data? {
        let file = fileURL(for: key)
        guard let data = try? Data(contentsOf: file) else { return nil }

        try? fileManager.setAttributes([.modificationDate: Date()], ofItemAtPath: file.path)
        return data
    }

    func store(_ data: Data, for key: String) {
        guard data.count <= maxBytes else { return }

        do {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
            try data.write(to: fileURL(for: key), options: .atomic)
            trim()
        } catch {
            return
        }
    }
}

private struct CachedFile {
    let url: URL
    let readAt: Date
    let size: Int
}

private extension ChatResourceDiskCache {
    func fileURL(for key: String) -> URL {
        let name = SHA256.hash(data: Data(key.utf8)).map { String(format: "%02x", $0) }.joined()
        return directory.appending(path: name, directoryHint: .notDirectory)
    }

    /// Removes the files that were read longest ago until the total size is within the bound.
    func trim() {
        let keys: [URLResourceKey] = [.contentModificationDateKey, .fileSizeKey]
        guard let files = try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: keys) else {
            return
        }

        let entries = files.compactMap { file -> CachedFile? in
            guard let values = try? file.resourceValues(forKeys: Set(keys)) else { return nil }
            return CachedFile(
                url: file,
                readAt: values.contentModificationDate ?? .distantPast,
                size: values.fileSize ?? 0
            )
        }
        .sorted { $0.readAt < $1.readAt }

        var total = entries.reduce(0) { $0 + $1.size }
        for entry in entries where total > maxBytes {
            try? fileManager.removeItem(at: entry.url)
            total -= entry.size
        }
    }
}
