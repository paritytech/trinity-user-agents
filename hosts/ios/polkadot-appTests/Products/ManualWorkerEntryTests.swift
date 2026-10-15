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

    private var productDirectory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Products")
            .appendingPathComponent(productId)
    }

    private func write(_ relativePath: String) throws {
        let file = productDirectory.appendingPathComponent(relativePath)
        try FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data([0x00]).write(to: file)
    }

    private func removeProduct() {
        try? FileManager.default.removeItem(at: productDirectory)
    }
}
