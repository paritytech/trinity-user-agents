import Foundation
import Products
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The bundled products listed with the core as funding providers: those
/// whose shipped Worker manifest includes Funding, with that manifest. One
/// that cannot be read is left out and logged.
struct BundledFundingProvidersTests {
    @Test
    func listsOnlyWorkersWhoseManifestIncludesFunding() throws {
        let root = try makeRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let funding = #"{"includes":{"funding":{}}}"#
        try write(funding, to: root, "ramp.dot/worker-manifest.json")
        try write("export {}", to: root, "ramp.dot/worker/index.js")
        try write(#"{"includes":{"chat":true}}"#, to: root, "chat.dot/worker-manifest.json")
        try write("export {}", to: root, "chat.dot/worker/index.js")
        try write(#"{"includes":{"funding":null}}"#, to: root, "null.dot/worker-manifest.json")
        try write("export {}", to: root, "null.dot/worker/index.js")
        try write(funding, to: root, "noworker.dot/worker-manifest.json")
        try write("export {}", to: root, "nomanifest.dot/worker/index.js")
        try write("not json", to: root, "broken.dot/worker-manifest.json")
        try write("export {}", to: root, "broken.dot/worker/index.js")

        let logger = MockLogger()
        let providers = BundledProducts(root: root).fundingProviders(logger: logger)

        #expect(try providers == [
            FundingProviderEntry(productId: ProductId("ramp.dot"), workerManifest: funding, origin: .bundled)
        ])
        #expect(logger.errors.count == 1)
        #expect(logger.errors.first?.contains("broken.dot") == true)
    }

    @Test
    func aBuildWithoutTheBundleListsNone() {
        #expect(BundledProducts.none.fundingProviders(logger: MockLogger()).isEmpty)
    }

    private func makeRoot() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bundled-funding-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    private func write(_ text: String, to root: URL, _ relativePath: String) throws {
        let url = root.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(text.utf8).write(to: url)
    }
}
