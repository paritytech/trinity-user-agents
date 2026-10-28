import Foundation

public struct DotNsLocalStorage {
    let content: DotNsContentStorageProtocol
    let contentHashes: ContentHashCacheProtocol

    public init(content: DotNsContentStorageProtocol, contentHashes: ContentHashCacheProtocol) {
        self.content = content
        self.contentHashes = contentHashes
    }
}
