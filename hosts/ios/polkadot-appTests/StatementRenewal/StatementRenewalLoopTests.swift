import Foundation
import StructuredConcurrencyTestSupport
import Testing

@testable import polkadot_app

@Suite(.timeLimit(.minutes(5)))
struct StatementRenewalLoopTests {
    @Test func renewsOnSetupAndAgainAfterEachInterval() async {
        let renewal = StatementDurabilityRenewalSpy()
        let clock = ManualClock()
        let loop = makeLoop(renewal: renewal, clock: clock)
        var events = renewal.events.makeAsyncIterator()

        loop.setup()
        #expect(await events.next() == .started)

        await clock.resumeSleep(for: StatementRenewalLoop.interval)
        #expect(await events.next() == .started)

        loop.throttle()
    }

    @Test func throttleCancelsTheRenewalInFlight() async {
        let renewal = StatementDurabilityRenewalSpy(holdsUntilCancelled: true)
        let loop = makeLoop(renewal: renewal, clock: ManualClock())
        var events = renewal.events.makeAsyncIterator()

        loop.setup()
        #expect(await events.next() == .started)

        loop.throttle()
        #expect(await events.next() == .cancelled)
    }
}

private extension StatementRenewalLoopTests {
    func makeLoop(renewal: StatementDurabilityRenewalSpy, clock: ManualClock) -> StatementRenewalLoop {
        StatementRenewalLoop(
            durabilityRenewal: renewal,
            backgroundExecutor: InlineBackgroundExecutor(),
            clock: clock
        )
    }
}
