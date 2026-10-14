import Foundation

public struct HandoffFileLoadConfig {
    public let chunkSize: Int

    /// Headroom subtracted from `chunkSize` for the inline threshold: covers the envelope's
    /// version and payload bytes, the compact length prefix, and the ChaCha20-Poly1305 nonce and tag,
    /// keeping the encrypted inline entry within the node-side limits accepted for chunks.
    public let inlineMargin: Int

    public var inlineThreshold: Int {
        chunkSize - inlineMargin
    }

    public init(chunkSize: Int = 2_000_000, inlineMargin: Int = 64) {
        self.chunkSize = chunkSize
        self.inlineMargin = inlineMargin
    }
}
