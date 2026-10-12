import Foundation

public typealias BlockWindowSink = (_ window: Data) throws -> Void
/// Sink the streaming CAR unpack pushes each reassembled file into.
public typealias CarContentWriter = (_ path: String, _ produce: (BlockWindowSink) throws -> Void) throws -> Void

public enum DotNsContentStorageError: Error {
    case pathEscapesContentDirectory(String)
    case invalidContentHash(String)
}

public protocol DotNsContentStorageProtocol {
    func saveContent(contentHash: String, write: (CarContentWriter) throws -> Void) throws
    func getContentDirectory(contentHash: String) -> URL?
    func contentExists(contentHash: String) -> Bool
    func deleteContent(contentHash: String) throws
    func loadContent(contentHash: String, relativePath: String) -> Data?
    func deleteAll() throws
}

public final class DotNsContentStorage: DotNsContentStorageProtocol {
    private let baseDirectory: URL
    private let fileManager: FileManager

    public init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        baseDirectory = appSupport.appendingPathComponent("DotNsContent")
    }

    // Stage into a temp location and atomically move it into place. This guarantees
    // getContentDirectory never observes a partially written bundle when another caller
    // (e.g. prewarm) is downloading the same content concurrently.
    public func saveContent(contentHash: String, write: (CarContentWriter) throws -> Void) throws {
        let contentDir = try contentDirectory(for: contentHash)

        guard !fileManager.fileExists(atPath: contentDir.path) else { return }

        try fileManager.createDirectory(at: baseDirectory, withIntermediateDirectories: true)

        let staging = baseDirectory.appendingPathComponent(".staging-\(UUID().uuidString)")

        do {
            try write { path, produce in
                try writeEntry(at: path, produce: produce, staging: staging)
            }
            try fileManager.moveItem(at: staging, to: contentDir)
        } catch {
            try? fileManager.removeItem(at: staging)

            // A concurrent writer may have produced the final content first, that's a success.
            if fileManager.fileExists(atPath: contentDir.path) {
                return
            }
            throw error
        }
    }

    public func getContentDirectory(contentHash: String) -> URL? {
        guard let contentDir = try? contentDirectory(for: contentHash) else { return nil }
        guard fileManager.fileExists(atPath: contentDir.path) else { return nil }
        return contentDir
    }

    public func contentExists(contentHash: String) -> Bool {
        guard let contentDir = try? contentDirectory(for: contentHash) else { return false }
        return fileManager.fileExists(atPath: contentDir.path)
    }

    public func loadContent(contentHash: String, relativePath: String) -> Data? {
        guard let contentDir = try? contentDirectory(for: contentHash),
              let fileURL = try? containedURL(base: contentDir, relativePath: relativePath) else {
            return nil
        }

        return fileManager.contents(atPath: fileURL.path)
    }

    public func deleteContent(contentHash: String) throws {
        let contentDir = try contentDirectory(for: contentHash)
        guard fileManager.fileExists(atPath: contentDir.path) else { return }
        try fileManager.removeItem(at: contentDir)
    }

    public func deleteAll() throws {
        guard fileManager.fileExists(atPath: baseDirectory.path) else { return }
        try fileManager.removeItem(at: baseDirectory)
    }
}

private extension DotNsContentStorage {
    /// Resolves `relativePath` against `base` and verifies the result stays inside it.
    func containedURL(base: URL, relativePath: String) throws -> URL {
        guard let contained = ContentArchivePath.inside(base, path: relativePath) else {
            throw DotNsContentStorageError.pathEscapesContentDirectory(relativePath)
        }

        return contained
    }

    /// `contentHash` is used directly as a path component, so it must be plain hex.
    func contentDirectory(for contentHash: String) throws -> URL {
        let isHex = !contentHash.isEmpty && contentHash.allSatisfy(\.isHexDigit)

        guard isHex else {
            throw DotNsContentStorageError.invalidContentHash(contentHash)
        }

        return baseDirectory.appendingPathComponent(contentHash)
    }

    func writeEntry(
        at path: String,
        produce: (BlockWindowSink) throws -> Void,
        staging: URL
    ) throws {
        var relativePath = path
        if relativePath.hasPrefix("/") {
            relativePath = String(relativePath.dropFirst())
        }

        if relativePath.isEmpty {
            // Single root file archive: the staging path itself is the file.
            // TODO: always store content in a dir
            try streamToFile(at: staging, produce: produce)
            return
        }

        if !fileManager.fileExists(atPath: staging.path) {
            try fileManager.createDirectory(at: staging, withIntermediateDirectories: true)
        }

        let fileURL = try containedURL(base: staging, relativePath: relativePath)
        let parentDir = fileURL.deletingLastPathComponent()

        if !fileManager.fileExists(atPath: parentDir.path) {
            try fileManager.createDirectory(at: parentDir, withIntermediateDirectories: true)
        }

        try streamToFile(at: fileURL, produce: produce)
    }

    func streamToFile(at url: URL, produce: (BlockWindowSink) throws -> Void) throws {
        fileManager.createFile(atPath: url.path, contents: nil)
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }

        try produce { window in
            try handle.write(contentsOf: window)
        }
    }
}
