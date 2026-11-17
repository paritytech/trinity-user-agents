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

    /// A bundled worker manifest goes through the parser a published one
    /// does, so the Worker comes with what it declares.
    @Test func readsItsWorkerThroughTheManifestParser() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try write(Self.workerManifest, to: root, "provider.dot/worker-manifest.json")
        try write("export {}", to: root, "provider.dot/worker/index.js")

        let product = try #require(BundledProducts(root: root).product(ProductId("provider.dot")))
        let worker = try #require(product.worker(screening: .passingThrough, logger: SilentLogger()))

        #expect(worker.identifier.rawValue == "provider.dot")
        #expect(worker.serves(.chat))
        #expect(!worker.serves(.pocket))
    }

    /// As for a published product, a manifest that is missing or rejected
    /// drops the Worker only: the App still ships.
    @Test(arguments: [nil, "{\"kind\":\"worker\"}", "not json"])
    func dropsOnlyTheWorkerWithoutAValidManifest(manifest: String?) throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        if let manifest {
            try write(manifest, to: root, "provider.dot/worker-manifest.json")
        }
        try write("export {}", to: root, "provider.dot/worker/index.js")
        try write("<html></html>", to: root, "provider.dot/app/index.html")

        let product = try #require(BundledProducts(root: root).product(ProductId("provider.dot")))

        #expect(product.worker(screening: .passingThrough, logger: SilentLogger()) == nil)
        #expect(product.hasApp())
    }

    /// A manifest without the Worker's script ships no Worker.
    @Test func shipsNoWorkerWithoutItsScript() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        try write(Self.workerManifest, to: root, "provider.dot/worker-manifest.json")

        let product = try #require(BundledProducts(root: root).product(ProductId("provider.dot")))

        #expect(product.worker(screening: .passingThrough, logger: SilentLogger()) == nil)
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

    /// A build made without the bundle still runs, with nothing bundled.
    @Test func shipsNothingWithoutARoot() {
        #expect(BundledProducts(root: nil).all.isEmpty)
        #expect(BundledProducts(root: URL(fileURLWithPath: "/nonexistent/BundledProducts.bundle")).all.isEmpty)
    }

    private static let workerManifest = """
    {"$v":1,"kind":"worker","appVersion":[1,0,0],"entrypoint":"index.js","includes":{"chat":true}}
    """

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
