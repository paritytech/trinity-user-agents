import Foundation

/// Describes when and where JavaScript is injected into web content.
public struct JSEngineScript: Sendable {
    /// Selects when the script is injected into a document.
    public enum InsertionPoint: Sendable {
        case atDocStart
        case atDocEnd
    }

    /// Selects which documents in the web view receive the script.
    ///
    /// The main frame is the top-level document. Other frames are documents
    /// loaded by `<iframe>` elements, each with its own JavaScript environment.
    public enum FrameScope: Equatable, Sendable {
        case mainFrameOnly
        case allFrames
    }

    public let content: String
    public let insertionPoint: InsertionPoint
    public let frameScope: FrameScope

    public init(
        content: String,
        insertionPoint: InsertionPoint,
        frameScope: FrameScope = .mainFrameOnly
    ) {
        self.content = content
        self.insertionPoint = insertionPoint
        self.frameScope = frameScope
    }
}
