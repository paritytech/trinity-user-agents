import Foundation

public protocol ChatProductFileProviding: ProductFileProviding {
    /// Entry module of a worker installed by hand through debug settings or the E2E harness, or nil
    /// when there is none. A wasm module wins over a script. Published workers declare their
    /// entrypoint in the manifest instead.
    func manualScriptEntryPath(productId: ProductId) -> String?

    /// When a worker installed by hand runs, from the `keepAlive` field of `ChatExtension/manifest.json`.
    /// `onDemand` when the file is absent, is not JSON, or has no value that this host knows.
    func manualWorkerKeepAlive(productId: ProductId) -> ProductWorkerKeepAlive
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

    public func manualWorkerKeepAlive(productId: ProductId) -> ProductWorkerKeepAlive {
        let path = chatScriptStorage.workerManifestRelativePath()
        guard
            let data = chatScriptStorage.loadData(productId: productId, relativePath: path),
            let manifest = try? JSONDecoder().decode(ManualWorkerManifestDTO.self, from: data) else {
            return .onDemand
        }

        return ProductWorkerKeepAlive(manifestValue: manifest.keepAlive?.value) ?? .onDemand
    }

    // MARK: - Private

    private func resolveContentHash(productId: ProductId) -> String? {
        contentHashCache.getContentHash(name: productId)
    }
}
