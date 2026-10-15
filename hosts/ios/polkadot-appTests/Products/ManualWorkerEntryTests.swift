import Foundation
import Products
import Testing

/// A worker installed by hand is a script or a wasm module, and the entry path tells the builder which file to
/// read.
@Suite(.serialized)
struct ManualWorkerEntryTests {
    private let storage = FileChatScriptStorage()
    private let productId = "manual-entry-\(UUID().uuidString.lowercased()).dot"

    private var provider: CompositeProductFileProvider {
        CompositeProductFileProvider(
            dotNsContentStorage: DotNsContentStorage(),
            chatScriptStorage: storage,
            contentHashCache: ContentHashCache()
        )
    }

    /// The E2E harness installs a wasm module beside no script, and the product must still serve chat.
    @Test
    func aWasmModuleIsAnEntry() throws {
        try write("ChatExtension/index.wasm")
        defer { removeProduct() }

        #expect(provider.manualScriptEntryPath(productId: productId) == "ChatExtension/index.wasm")
    }

    /// A module wins over a script left by an earlier install, so the harness need not remove the script first.
    @Test
    func aWasmModuleWinsOverAScript() throws {
        try write("ChatExtension/index.js")
        try write("ChatExtension/index.wasm")
        defer { removeProduct() }

        #expect(provider.manualScriptEntryPath(productId: productId) == "ChatExtension/index.wasm")
    }

    @Test
    func aScriptAloneStaysTheEntry() throws {
        try write("ChatExtension/index.js")
        defer { removeProduct() }

        #expect(provider.manualScriptEntryPath(productId: productId) == "ChatExtension/index.js")
    }

    @Test
    func noFileIsNoEntry() {
        #expect(provider.manualScriptEntryPath(productId: productId) == nil)
    }

    /// The harness declares a foreground worker in `ChatExtension/manifest.json`, as a published worker does in
    /// its manifest (I5).
    @Test
    func aManifestBesideTheEntryDeclaresAForegroundWorker() throws {
        try write("ChatExtension/index.wasm")
        try write("ChatExtension/manifest.json", #"{"keepAlive":"foreground"}"#)
        defer { removeProduct() }

        #expect(provider.manualWorkerKeepAlive(productId: productId) == .foreground)
    }

    /// No file, a file that is not JSON, and a value that this host does not know all keep the worker on demand.
    @Test(arguments: [nil, "not json", #"{"keepAlive":"always"}"#, #"{"keepAlive":"onDemand"}"#, "{}"])
    func anyOtherManifestKeepsTheWorkerOnDemand(_ manifest: String?) throws {
        try write("ChatExtension/index.wasm")
        if let manifest { try write("ChatExtension/manifest.json", manifest) }
        defer { removeProduct() }

        #expect(provider.manualWorkerKeepAlive(productId: productId) == .onDemand)
    }

    private var productDirectory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Products")
            .appendingPathComponent(productId)
    }

    private func write(_ relativePath: String, _ text: String? = nil) throws {
        let file = productDirectory.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try (text.map { Data($0.utf8) } ?? Data([0x00])).write(to: file)
    }

    private func removeProduct() {
        try? FileManager.default.removeItem(at: productDirectory)
    }
}
