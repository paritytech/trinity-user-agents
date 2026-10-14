import Foundation

public protocol ChatScriptStorageProtocol: ProductFileProviding {
    func saveScript(productId: ProductId, content: String) throws
    func loadScript(productId: ProductId) -> String?
    func deleteScript(productId: ProductId) throws
    func scriptExists(productId: ProductId) -> Bool
    func loadData(productId: ProductId, relativePath: String) -> Data?
    func chatEntrypointRelativePath() -> String
}

public extension ChatScriptStorageProtocol {
    func load(for productId: ProductId, relativePath: String) -> Data? {
        loadData(productId: productId, relativePath: relativePath)
    }
}

public final class FileChatScriptStorage: ChatScriptStorageProtocol {
    private let fileManager: FileManager
    private let baseDirectory: URL

    public init(fileManager: FileManager = .default) {
        self.fileManager = fileManager
        baseDirectory = Self.resolveBaseDirectory(fileManager: fileManager)
    }

    public func saveScript(productId: ProductId, content: String) throws {
        try ensureDirectoryExists(productId: productId)
        let fileURL = scriptURL(for: productId)
        try content.write(to: fileURL, atomically: true, encoding: .utf8)
    }

    public func loadScript(productId: ProductId) -> String? {
        let fileURL = scriptURL(for: productId)
        return try? String(contentsOf: fileURL, encoding: .utf8)
    }

    public func deleteScript(productId: ProductId) throws {
        let fileURL = scriptURL(for: productId)

        guard fileManager.fileExists(atPath: fileURL.path) else { return }

        try fileManager.removeItem(at: fileURL)
    }

    public func scriptExists(productId: ProductId) -> Bool {
        let fileURL = scriptURL(for: productId)
        return fileManager.fileExists(atPath: fileURL.path)
    }

    public func loadData(productId: ProductId, relativePath: String) -> Data? {
        let fileURL = baseDirectory
            .appendingPathComponent(productId)
            .appendingPathComponent(relativePath)
        return fileManager.contents(atPath: fileURL.path)
    }

    public func chatEntrypointRelativePath() -> String {
        "ChatExtension/index.js"
    }

    // MARK: - Private

    private func scriptURL(for productId: ProductId) -> URL {
        baseDirectory.appendingPathComponent("\(productId)/\(chatEntrypointRelativePath())")
    }

    private func ensureDirectoryExists(productId: ProductId) throws {
        let scriptUrl = scriptURL(for: productId)
        let dir = scriptUrl.deletingLastPathComponent()

        guard !fileManager.fileExists(atPath: dir.path) else { return }

        try fileManager.createDirectory(
            at: dir,
            withIntermediateDirectories: true
        )
    }

    private static func resolveBaseDirectory(fileManager: FileManager) -> URL {
        let appSupportURL = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return appSupportURL.appendingPathComponent("Products")
    }
}
