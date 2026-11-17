import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// A card decides once what it loads, so a bundled product's card never asks
/// dotNS for the App the app already ships.
struct ProductSourcesTests {
    private func makeKey() throws -> PocketCardKey {
        try PocketCardKey(productId: ProductId("provider.dot"), cardId: PocketCardId(value: "cash"))
    }

    @Test
    func loadsTheAppABundledProductShips() throws {
        let key = try makeKey()
        let root = try BundledFixture.makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try BundledFixture.write("<html></html>", to: root, "provider.dot/app/index.html")

        let content = ProductSources(bundledProducts: BundledProducts(root: root))
            .cardContent(for: key, pageQuery: "")

        guard case let .bundled(directory) = content else {
            Issue.record("expected the bundled App, got \(content)")
            return
        }
        #expect(directory.standardizedFileURL == root.appendingPathComponent("provider.dot/app").standardizedFileURL)
    }

    @Test
    func loadsDotNsForAProductNotShipped() throws {
        let key = try makeKey()
        let content = ProductSources(bundledProducts: .none).cardContent(for: key, pageQuery: "")

        #expect(content.isDotNs)
    }

    /// A bundled product without an App has no page of its own to serve.
    @Test
    func loadsDotNsForABundledProductWithoutAnApp() throws {
        let key = try makeKey()
        let root = try BundledFixture.makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try BundledFixture.write("export {}", to: root, "provider.dot/worker/index.js")

        let content = ProductSources(bundledProducts: BundledProducts(root: root))
            .cardContent(for: key, pageQuery: "")

        #expect(content.isDotNs)
    }

    /// A widget address typed in debug settings wins over the bundle.
    @Test
    func loadsTheDebugWidgetFirst() throws {
        let key = try makeKey()
        let root = try BundledFixture.makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try BundledFixture.write("<html></html>", to: root, "provider.dot/app/index.html")
        let widget = try #require(URL(string: "http://localhost:5173/widget?q=1"))

        let content = ProductSources(
            bundledProducts: BundledProducts(root: root),
            debugWidgetURL: { _, pageQuery in pageQuery == "q=1" ? widget : nil }
        ).cardContent(for: key, pageQuery: "q=1")

        guard case let .directURL(url) = content else {
            Issue.record("expected the debug widget, got \(content)")
            return
        }
        #expect(url == widget)
    }

    @Test
    func decidesTheWorkerFromTheBundledManifest() throws {
        let key = try makeKey()
        let root = try BundledFixture.makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try BundledFixture.write(BundledFixture.workerManifest, to: root, "provider.dot/worker-manifest.json")
        try BundledFixture.write("export {}", to: root, "provider.dot/worker/index.js")

        let source = ProductSources(bundledProducts: BundledProducts(root: root)).source(of: key.productId)

        guard case let .bundled(product, worker) = source else {
            Issue.record("expected a bundled source, got \(source)")
            return
        }
        #expect(product.id == key.productId)
        #expect(worker?.serves(.chat) == true)
    }
}

enum BundledFixture {
    static let workerManifest = """
    {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":true}}
    """

    static func makeRoot() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bundled-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    static func write(_ text: String, to root: URL, _ relativePath: String) throws {
        let url = root.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(text.utf8).write(to: url)
    }
}

private extension SPAContentSource {
    var isDotNs: Bool {
        if case .dotNs = self { true } else { false }
    }
}
