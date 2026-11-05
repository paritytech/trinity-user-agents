import Foundation
import Testing
import TrUAPIHost
@testable import Products

struct BundledProductsTests {
    @Test func readsEachProductUnderTheRoot() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try write("{\"kind\":\"worker\"}", to: root, "provider.dot/worker-manifest.json")
        try write("export {}", to: root, "provider.dot/worker/index.js")
        try write("<html></html>", to: root, "provider.dot/app/index.html")
        try write("ignored", to: root, ".staging/worker/index.js")

        let id = try ProductId("provider.dot")
        let products = BundledProducts(root: root)
        let product = try #require(products.product(id))

        #expect(products.all.map(\.id.rawValue) == ["provider.dot"])
        #expect(product.workerManifest() == "{\"kind\":\"worker\"}")
        #expect(product.hasWorker())
        #expect(product.hasApp())
        #expect(ProductWorkerSource.bundled(product) == ProductWorkerSource(
            contentId: id,
            entryRelativePath: "worker/index.js"
        ))
    }

    /// The app's network has its own top-level domain, which a bundled id
    /// need not end in, and a provider's screen must still resolve.
    @Test func opensPagesUnderItsOwnIdOnAnyNetwork() throws {
        let product = try BundledProduct(
            id: ProductId("provider.dot"),
            directory: URL(fileURLWithPath: "/bundle/provider.dot")
        )
        let page = try #require(product.page(navigationDestination: "provider.dot/card?intent=fs_1"))

        #expect(page.host.toDotDomain().rawValue == "provider.dot")
        #expect(page.page == "/card?intent=fs_1")
    }

    /// Funding is served by a manifest whose `includes` has a `funding`
    /// section; a null one serves nothing.
    @Test func servesFundingOnlyWithAFundingSection() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let funding = #"{"includes":{"funding":{}}}"#
        try write(funding, to: root, "ramp.dot/worker-manifest.json")
        try write(#"{"includes":{"funding":null}}"#, to: root, "null.dot/worker-manifest.json")
        try write(#"{"includes":{"chat":true}}"#, to: root, "chat.dot/worker-manifest.json")
        try write("not json", to: root, "broken.dot/worker-manifest.json")

        let products = BundledProducts(root: root)

        #expect(try products.product(ProductId("ramp.dot"))?.fundingWorkerManifest() == funding)
        #expect(try products.product(ProductId("null.dot"))?.fundingWorkerManifest() == nil)
        #expect(try products.product(ProductId("chat.dot"))?.fundingWorkerManifest() == nil)
        #expect(try products.product(ProductId("broken.dot"))?.fundingWorkerManifest() == nil)
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
