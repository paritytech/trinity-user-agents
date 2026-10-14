import Foundation

/// Product icon deployed on the Bulletin chain and addressed by CID.
public struct ProductIcon: Hashable, Sendable {
    public let cid: String
    public let format: Format

    public enum Format: String, Sendable {
        case jpeg
        case png
    }

    public init(cid: String, format: Format) {
        self.cid = cid
        self.format = format
    }
}
