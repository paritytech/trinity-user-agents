import Foundation

public protocol ChatProductFileProviding: ProductFileProviding {
    /// Entry module of a worker installed by hand through debug settings or the E2E harness, or nil
    /// when there is none. A wasm module wins over a script. Published workers declare their
    /// entrypoint in the manifest instead.
    func manualScriptEntryPath(productId: ProductId) -> String?
}

public final class CompositeProductFileProvider: ChatProductFileProviding {
    private let dotNsContentStorage: DotNsContentStorageProtocol
    private let chatScriptStorage: ChatScriptStorageProtocol
    private let contentHashCache: ContentHashCacheProtocol

    public init(
        dotNsContentStorage: DotNsContentStorageProtocol,
        chatScriptStorage: ChatScriptStorageProtocol,
        contentHashCache: ContentHashCacheProtocol
    ) {
        self.dotNsContentStorage = dotNsContentStorage
        self.chatScriptStorage = chatScriptStorage
        self.contentHashCache = contentHashCache
    }

    public func load(for productId: ProductId, relativePath: String) -> Data? {
        guard let contentHash = resolveContentHash(productId: productId),
              let data = dotNsContentStorage.loadContent(contentHash: contentHash, relativePath: relativePath)
        else {
            return chatScriptStorage.loadData(productId: productId, relativePath: relativePath)
        }

        return data
    }

    public func manualScriptEntryPath(productId: ProductId) -> String? {
        let wasmEntry = chatScriptStorage.wasmEntrypointRelativePath()
        if chatScriptStorage.fileExists(productId: productId, relativePath: wasmEntry) {
            return wasmEntry
        }

        guard chatScriptStorage.scriptExists(productId: productId) else { return nil }

        return chatScriptStorage.chatEntrypointRelativePath()
    }

    // MARK: - Private

    private func resolveContentHash(productId: ProductId) -> String? {
        contentHashCache.getContentHash(name: productId)
    }
}
