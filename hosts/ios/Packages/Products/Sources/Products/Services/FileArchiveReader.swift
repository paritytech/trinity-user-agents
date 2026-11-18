import Foundation
import TrUAPIHost

/// An archive on disk as the core reads it, by absolute path.
final class FileArchiveReader: BundleReader {
    func list(dir: String) -> [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? []
    }

    /// A regular file only, as the core expects: a directory is served by
    /// its `index.html`, and an archive root that is a directory is not a
    /// single-file archive.
    func exists(path: String) -> Bool {
        var isDirectory: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory) && !isDirectory.boolValue
    }

    func readText(path: String) -> String? {
        FileManager.default.contents(atPath: path).flatMap { String(data: $0, encoding: .utf8) }
    }
}
