import Foundation

/// Where a card's static face is read from.
///
/// A published manifest can only ever name ``archive(path:)``: letting it name a
/// ``url(_:)`` would hand any product on chain a way to make the host fetch an
/// address of its choosing, before the user has approved anything. The URL case
/// exists for a worker supplied through the debug menu, which is not published.
public enum PocketCardPreview: Hashable, Sendable {
    case archive(path: String)
    case url(String)
}

/// A card a worker manifest publishes.
public struct PocketCardDefinition: Hashable, Sendable {
    public let id: PocketCardId
    public let title: String
    public let preview: PocketCardPreview

    public init(id: PocketCardId, title: String, preview: PocketCardPreview) {
        self.id = id
        self.title = title
        self.preview = preview
    }
}
