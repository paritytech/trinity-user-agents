import Foundation

/// The tap that opened a product page, read before any hop so the page can measure from it.
public struct HostOpenTap: Sendable, Equatable {
    public enum Source: String, Sendable {
        case pocketCard = "pocket_card"
        case deeplink
    }

    public let openedAtEpochMilliseconds: Int
    public let source: Source

    public init(source: Source, openedAt: Date = Date()) {
        self.source = source
        openedAtEpochMilliseconds = openedAt.epochMilliseconds
    }
}

extension Date {
    /// Whole milliseconds since 1970, rounded so a fractional second does not lose one to the binary float.
    var epochMilliseconds: Int {
        Int((timeIntervalSince1970 * 1000).rounded())
    }
}
