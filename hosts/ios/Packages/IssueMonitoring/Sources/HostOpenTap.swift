import Foundation

/// The tap that opened a product page, read before any hop so the page can measure from it.
public struct HostOpenTap: Sendable, Equatable {
    public enum Source: String, Sendable {
        case pocketCard = "pocket_card"
        case deeplink
    }

    /// The tap's wall clock, which the page reads to measure on its own clock.
    public let openedAtEpochMilliseconds: Int
    /// The tap on `HostEventClock`, which the host measures `tap_ms` from, so a wall clock correction between the
    /// tap and the page cannot shorten or lengthen the open.
    public let openedAtMonotonicMilliseconds: Int
    public let source: Source
    /// Minted at the tap; the session that takes this tap adopts it as its host session id.
    public let sessionID: UUID

    public init(
        source: Source,
        openedAt: Date = Date(),
        openedAtMonotonicMilliseconds: Int = HostEventClock.monotonicMilliseconds(),
        sessionID: UUID = UUID()
    ) {
        self.source = source
        self.sessionID = sessionID
        self.openedAtMonotonicMilliseconds = openedAtMonotonicMilliseconds
        openedAtEpochMilliseconds = openedAt.epochMilliseconds
    }

    private init(
        source: Source,
        openedAtEpochMilliseconds: Int,
        openedAtMonotonicMilliseconds: Int,
        sessionID: UUID
    ) {
        self.source = source
        self.openedAtEpochMilliseconds = openedAtEpochMilliseconds
        self.openedAtMonotonicMilliseconds = openedAtMonotonicMilliseconds
        self.sessionID = sessionID
    }

    /// The host's share of the open from the tap to `monotonicMilliseconds`, read on the same clock as the tap.
    func milliseconds(until monotonicMilliseconds: Int) -> Int {
        monotonicMilliseconds - openedAtMonotonicMilliseconds
    }

    /// The same tap, joined to the session of a page it showed again rather than built, so its events and that
    /// page's events share one host session id.
    public func showing(sessionID: UUID) -> HostOpenTap {
        HostOpenTap(
            source: source,
            openedAtEpochMilliseconds: openedAtEpochMilliseconds,
            openedAtMonotonicMilliseconds: openedAtMonotonicMilliseconds,
            sessionID: sessionID
        )
    }
}

extension Date {
    /// Whole milliseconds since 1970, rounded so a fractional second does not lose one to the binary float.
    var epochMilliseconds: Int {
        Int((timeIntervalSince1970 * 1000).rounded())
    }
}
