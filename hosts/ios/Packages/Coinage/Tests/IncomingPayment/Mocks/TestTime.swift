import Clocks
import Foundation
import FoundationExt

/// One virtual time for a service that paces itself on a `Clock` and checks deadlines against `Date`:
/// the date is derived from the test clock, so advancing the clock moves both together and nothing in
/// a test waits on the wall clock.
final class TestTime: @unchecked Sendable {
    struct Stalled: Error {
        let advanced: Duration
    }

    let clock = TestClock<Duration>()

    private let start = Date()
    private let epoch: TestClock<Duration>.Instant

    init() {
        epoch = clock.now
    }

    /// The wall-clock date matching the clock's current instant.
    var now: Date {
        start.addingTimeInterval(epoch.duration(to: clock.now).timeInterval)
    }

    /// The date `interval` seconds after the clock's origin, for deadlines handed to the code under test.
    func date(after interval: TimeInterval) -> Date {
        start.addingTimeInterval(interval)
    }

    func advance(by duration: Duration) async {
        await clock.advance(by: duration)
    }

    /// Advances in `step`s until `condition` holds. Stepping, rather than one jump, lets the code under
    /// test schedule its next sleep between steps, so a sleep registered after a jump is never stranded
    /// beyond it. Throws once `limit` of virtual time has passed without the condition holding.
    func advance(
        until condition: @escaping @Sendable () -> Bool,
        step: Duration = .milliseconds(10),
        limit: Duration = .seconds(10)
    ) async throws {
        var advanced: Duration = .zero
        while !condition() {
            guard advanced < limit else { throw Stalled(advanced: advanced) }
            await clock.advance(by: step)
            advanced += step
        }
    }
}
