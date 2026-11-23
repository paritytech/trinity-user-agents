import Foundation
import UniformTypeIdentifiers

/// The `Content-Type` a product file is served with.
enum ProductContentType {
    static let unknown = "application/octet-stream"

    /// Keeps a specific type the caller already has and falls back to the
    /// system's type for the file's extension, since the core only types a
    /// handful of web extensions.
    static func of(path: String, known: String? = nil) -> String {
        if let known, !known.isEmpty, known != unknown {
            return known
        }

        let pathExtension = (path as NSString).pathExtension
        return UTType(filenameExtension: pathExtension)?.preferredMIMEType ?? unknown
    }
}
