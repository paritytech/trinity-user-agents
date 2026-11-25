import Foundation
import FoundationExt
import StructuredConcurrencyTestSupport
import Testing
@testable import polkadot_app

/// `fires(at:)` paces both transfer-monitor windows; a deadline that fires early, twice or never
/// would flip a payment's row at the wrong time or keep its task alive for ever.
@Suite("Time source deadline", .timeLimit(.minutes(1)))
struct TimeSourceTests {
    private let start = Date(timeIntervalSince1970: 1_700_000_000)
    private let delay: TimeInterval = 60

    @Test("a future deadline fires once, when the date has reached it, then finishes")
    func firesOnceAtTheDeadline() async {
        let clock = ManualClock()
        let dateProvider = MockDateProvider(now: start)
        let source = TimeSource(clock: clock, dateProvider: dateProvider)
        var events = source.fires(at: start.addingTimeInterval(delay)).makeAsyncIterator()

        await clock.waitForSleep(for: .seconds(delay))
        await dateProvider.advance(by: delay)
        await clock.resumeSleep(for: .seconds(delay))

        let fired: Void? = await events.next()
        #expect(fired != nil)
        let afterwards: Void? = await events.next()
        #expect(afterwards == nil)
    }

    @Test("a deadline already passed fires without sleeping")
    func pastDeadlineFiresAtOnce() async {
        let source = TimeSource(clock: ManualClock(), dateProvider: MockDateProvider(now: start))
        var events = source.fires(at: start.addingTimeInterval(-1)).makeAsyncIterator()

        let fired: Void? = await events.next()
        #expect(fired != nil)
        let afterwards: Void? = await events.next()
        #expect(afterwards == nil)
    }

    @Test("a clock running ahead of the date re-reads the date and sleeps the remainder")
    func clockAheadOfDateSleepsAgain() async {
        let clock = ManualClock()
        let dateProvider = MockDateProvider(now: start)
        let source = TimeSource(clock: clock, dateProvider: dateProvider)
        var events = source.fires(at: start.addingTimeInterval(delay)).makeAsyncIterator()

        await clock.waitForSleep(for: .seconds(delay))
        await dateProvider.advance(by: delay / 2)
        await clock.resumeSleep(for: .seconds(delay))
        await clock.waitForSleep(for: .seconds(delay / 2))
        await dateProvider.advance(by: delay / 2)
        await clock.resumeSleep(for: .seconds(delay / 2))

        let fired: Void? = await events.next()
        #expect(fired != nil)
    }

    @Test("a consumer that stops listening ends the stream without a fire")
    func terminationEndsWithoutFiring() async {
        let clock = ManualClock()
        let source = TimeSource(clock: clock, dateProvider: MockDateProvider(now: start))
        let consumer = Task { () -> Bool in
            for await _ in source.fires(at: start.addingTimeInterval(delay)) {
                return true
            }
            return false
        }

        await clock.waitForSleep(for: .seconds(delay))
        consumer.cancel()

        #expect(await consumer.value == false)
    }
}
