import Foundation
import Testing
import TrUAPIHost
@testable import Products

struct BundledProductsTests {
    @Test func readsEachProductUnderTheRoot() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try write("{\"$v\":1,\"displayName\":\"Provider\"}", to: root, "provider.dot/manifest.json")
        try write("export {}", to: root, "provider.dot/worker/index.js")
        try write("<html></html>", to: root, "provider.dot/app/index.html")
        try write("ignored", to: root, ".staging/worker/index.js")

        let id = try ProductId("provider.dot")
        let products = BundledProducts(root: root)
        let entry = try #require(products.entries.first)

        #expect(products.entries.map(\.productId) == [id])
        #expect(entry.hasWorker)
        #expect(entry.hasApp)
        #expect(entry.displayName == "Provider")
        #expect(products.source(of: id) == .bundled(entry: entry))
        #expect(ProductWorkerSource.bundled(entry) == ProductWorkerSource(
            contentId: id,
            entryRelativePath: "worker/index.js"
        ))
    }

    /// The app's network has its own top-level domain, which a bundled id
    /// need not end in, and a provider's screen must still resolve.
    @Test func opensPagesUnderItsOwnIdOnAnyNetwork() throws {
        let entry = try BundleEntry(
            productId: ProductId("provider.dot"),
            path: "/bundle/provider.dot",
            hasWorker: true,
            hasApp: true,
            displayName: nil,
            iconCid: nil
        )
        let page = try #require(entry.page(navigationDestination: "provider.dot/card?intent=fs_1"))

        #expect(page.host.toDotDomain().rawValue == "provider.dot")
        #expect(page.page == "/card?intent=fs_1")
    }

    /// A build made without the bundle still runs, with nothing bundled.
    @Test func shipsNothingWithoutARoot() {
        #expect(BundledProducts(root: nil).entries.isEmpty)
        #expect(BundledProducts(root: URL(fileURLWithPath: "/nonexistent/BundledProducts.bundle")).entries.isEmpty)
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
