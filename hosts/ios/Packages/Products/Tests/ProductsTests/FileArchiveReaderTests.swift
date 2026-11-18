import Foundation
import Testing
import TrUAPIHost
@testable import Products

struct FileArchiveReaderTests {
    /// An App's route that names a directory loads that directory's page,
    /// and the App's root is a directory, not a single-file archive.
    @Test func servesARouteFromItsDirectoryPage() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try write("<html>start</html>", to: root, "app/index.html")
        try write("<html>card</html>", to: root, "app/card/index.html")
        let app = root.appendingPathComponent("app").path

        let card = archiveFile(root: app, requestPath: "/card", isMainFrame: true, reader: FileArchiveReader())
        let start = archiveFile(root: app, requestPath: "/", isMainFrame: true, reader: FileArchiveReader())

        #expect(card?.path == app + "/card/index.html")
        #expect(start?.path == app + "/index.html")
        #expect(!FileArchiveReader().exists(path: app))
    }

    private func write(_ text: String, to root: URL, _ relativePath: String) throws {
        let url = root.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(text.utf8).write(to: url)
    }
}
