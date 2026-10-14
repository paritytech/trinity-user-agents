import Foundation

/// Result of parsing a CAR archive: a mapping from relative file paths to file content.
public struct UnpackedArchive {
    /// File path → file content.
    public let files: [String: Data]

    public init(files: [String: Data]) {
        self.files = files
    }
}
