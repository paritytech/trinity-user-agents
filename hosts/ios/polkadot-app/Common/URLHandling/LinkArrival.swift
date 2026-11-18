import Foundation
import FoundationExt
import IssueMonitoring

/// When a link reached the app, read once where the system handed it over. A link can wait before any
/// handler takes it (a cold start's waits until the main screen registers its handlers), and whatever it
/// opens is timed from this reading, not from when the wait ended.
struct LinkArrival: Sendable, Equatable {
    /// The wall clock, which a page reads to measure on its own clock.
    let date: Date
    /// The reading on `HostEventClock`, which the host measures `tap_ms` on.
    let monotonicMilliseconds: Int
}

protocol LinkArrivalClock: Sendable {
    func read() -> LinkArrival
}

/// Reads the wall clock from a `CurrentDateProviding`, synchronously, so the reading is taken where the link
/// arrives and not after a hop.
struct SystemLinkArrivalClock: LinkArrivalClock {
    var dateProvider: CurrentDateProviding = NowDateProvider()

    func read() -> LinkArrival {
        LinkArrival(date: dateProvider.now, monotonicMilliseconds: HostEventClock.monotonicMilliseconds())
    }
}
