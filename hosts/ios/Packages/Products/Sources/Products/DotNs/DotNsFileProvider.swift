import Foundation

public final class DotNsFileProvider: ProductFileProviding {
    // TODO: Should be contentDirectory
    private let contentURL: URL
    private let fileManager: FileManager

    public init(contentURL: URL, fileManager: FileManager = .default) {
        self.contentURL = contentURL
        self.fileManager = fileManager
    }

    public func load(for _: ProductId, relativePath: String) -> Data? {
        var isDirectory: ObjCBool = false

        guard fileManager.fileExists(
            atPath: contentURL.path,
            isDirectory: &isDirectory
        ) else {
            return nil
        }

        guard isDirectory.boolValue else {
            return fileManager.contents(atPath: contentURL.path)
        }

        let fileURL = contentURL.appendingPathComponent(relativePath)
        return fileManager.contents(atPath: fileURL.path)
    }
}
