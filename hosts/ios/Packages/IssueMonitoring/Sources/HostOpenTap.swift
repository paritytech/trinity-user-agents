import Foundation

/// The tap that opened a product page, read before any hop so the page can measure from it.
public struct HostOpenTap: Sendable, Equatable {
    public enum Source: String, Sendable {
        case pocketCard = "pocket_card"
        case deeplink
    }

    public let openedAtEpochMilliseconds: Int
    public let source: Source
    /// Minted at the tap; the session that takes this tap adopts it as its host session id.
    public let sessionID: UUID

    public init(source: Source, openedAt: Date = Date(), sessionID: UUID = UUID()) {
        self.source = source
        self.sessionID = sessionID
        openedAtEpochMilliseconds = openedAt.epochMilliseconds
    }
}

extension Date {
    /// Whole milliseconds since 1970, rounded so a fractional second does not lose one to the binary float.
    var epochMilliseconds: Int {
        Int((timeIntervalSince1970 * 1000).rounded())
    }
}
