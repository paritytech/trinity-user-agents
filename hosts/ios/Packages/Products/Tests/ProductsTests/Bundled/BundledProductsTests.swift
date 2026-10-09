import Foundation
import Testing
@testable import Products

struct BundledProductsTests {
    @Test func readsEachProductUnderTheRoot() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try write("{\"kind\":\"worker\"}", to: root, "provider.dot/worker-manifest.json")
        try write("export {}", to: root, "provider.dot/worker/index.js")
        try write("<html></html>", to: root, "provider.dot/app/index.html")
        try write("ignored", to: root, ".staging/worker/index.js")

        let products = BundledProducts(root: root)
        let product = try #require(products.product("provider.dot"))

        #expect(products.all.map(\.id) == ["provider.dot"])
        #expect(product.workerManifest() == "{\"kind\":\"worker\"}")
        #expect(product.hasWorker())
        #expect(product.hasApp())
        #expect(ProductWorkerSource.bundled(product) == ProductWorkerSource(
            contentId: "provider.dot",
            entryRelativePath: "worker/index.js"
        ))
    }

    /// A build made without the bundle still runs, with nothing bundled.
    @Test func shipsNothingWithoutARoot() {
        #expect(BundledProducts(root: nil).all.isEmpty)
        #expect(BundledProducts(root: URL(fileURLWithPath: "/nonexistent/BundledProducts.bundle")).all.isEmpty)
    }

    private func makeRoot() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func write(_ text: String, to root: URL, _ relativePath: String) throws {
        let url = root.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(text.utf8).write(to: url)
    }
}
