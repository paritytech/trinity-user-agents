import AsyncExtensions
import Foundation
import FoundationExt
import Testing
@testable import Coinage

/// The gate every rebuild waits behind. Two things have to hold: a look never claims an input is absent
/// unless one was actually taken, and the newest look always wins — a fork can take an input away, and
/// building against the widest view ever seen would spend what is no longer there.
@Suite("Await Inputs")
struct AwaitInputsTests {
    private let now = Date(timeIntervalSince1970: 1_700_000_000)

    @Test("a look holding every input returns it")
    func everyInputPresent() async {
        let look = await awaitInputs(
            presence: stream([[1, 2]]),
            wanted: [1, 2],
            deadline: now.addingTimeInterval(60),
            timing: timing()
        )

        #expect(look.looked)
        #expect(look.present == [1, 2])
    }

    @Test("inputs outside the wanted set are ignored")
    func extraInputsAreIgnored() async {
        let look = await awaitInputs(
            presence: stream([[1, 2, 99]]),
            wanted: [1, 2],
            deadline: now.addingTimeInterval(60),
            timing: timing()
        )

        #expect(look.present == [1, 2])
    }

    @Test("a partial look holds out, and takes the completing one")
    func partialLookHoldsOutForTheRest() async {
        // A long hold-out costs nothing here — the wait ends the moment every input is visible — and it
        // keeps a stalled pump from being read as "input 2 never arrived".
        let look = await awaitInputs(
            presence: stream([[1], [1, 2]]),
            wanted: [1, 2],
            deadline: now.addingTimeInterval(60),
            timing: timing()
        )

        #expect(look.present == [1, 2])
    }

    @Test("the newest look wins even when it holds fewer inputs")
    func newestLookWinsAfterAFork() async {
        // A fork takes input 2 back after it was first seen; building on the wider view would spend
        // something the chain no longer holds.
        //
        // The stream finishes rather than settling, so the hold-out ends on the stream and not on the
        // clock: which look wins is then a fact about the code, not about how fast the machine ran.
        let look = await awaitInputs(
            presence: stream([[1, 2], [1]]),
            wanted: [1, 2, 3],
            deadline: now.addingTimeInterval(60),
            timing: timing()
        )

        #expect(look.present == [1])
    }

    @Test("a partial look that never completes is settled for once the hold-out passes")
    func holdOutSettlesForThePartialLook() async {
        // Deliberately short, and the exception to the generous budgets everywhere else here: this is the
        // one test whose *subject* is the hold-out expiring, so the wait is the behaviour rather than a
        // safety margin and the suite pays it on every run. Lengthening it would buy no reliability — the
        // only look taken is already the answer, so the outcome cannot change however slowly it runs — and
        // would cost that much wall time each time.
        let look = await awaitInputs(
            presence: stream([[1]], settling: true),
            wanted: [1, 2],
            deadline: now.addingTimeInterval(60),
            timing: timing(holdOut: .milliseconds(50))
        )

        #expect(look.looked)
        #expect(look.present == [1])
    }

    @Test("a look taken while nothing is present is still a look")
    func emptyLookPastDeadlineCounts() async {
        // The window is already closed, so the first empty look is the last word.
        let look = await awaitInputs(
            presence: stream([[]]),
            wanted: [1],
            deadline: now.addingTimeInterval(-1),
            timing: timing()
        )

        #expect(look.looked)
        #expect(look.present.isEmpty)
        #expect(look.abandoned(1, deadline: now.addingTimeInterval(-1)))
    }

    @Test("a subscription that never yields proves nothing absent")
    func noLookProvesNothing() async {
        let look = await awaitInputs(
            presence: AsyncStream<Set<Int>> { _ in }.eraseToAnyAsyncSequence(),
            wanted: [1],
            deadline: now.addingTimeInterval(-1),
            // Short for the same reason as the hold-out above: the idle limit expiring *is* what is being
            // tested, so it is paid on every run. Nothing can ever be emitted here, so no budget could
            // make this conclude differently — only take longer.
            timing: timing(idleLimit: .milliseconds(20))
        )

        #expect(!look.looked)
        #expect(!look.abandoned(1, deadline: now.addingTimeInterval(-1)))
    }

    @Test("a failing subscription proves nothing absent")
    func failedReadProvesNothing() async {
        let failing = AsyncThrowingStream<Set<Int>, Error> { $0.finish(throwing: PresenceReadFailure()) }

        let look = await awaitInputs(
            presence: failing.eraseToAnyAsyncSequence(),
            wanted: [1],
            deadline: now.addingTimeInterval(-1),
            timing: timing()
        )

        #expect(!look.looked)
    }

    @Test("an input absent from a look taken before its deadline is not abandoned")
    func absenceBeforeDeadlineIsNotAbandonment() async {
        let look = await awaitInputs(
            presence: stream([[1]]),
            wanted: [1, 2],
            deadline: now.addingTimeInterval(60),
            timing: timing()
        )

        #expect(look.looked)
        #expect(!look.present.contains(2))
        // The look was taken well before the deadline, so input 2 may still arrive.
        #expect(!look.abandoned(2, deadline: now.addingTimeInterval(60)))
    }
}

/// A presence subscription that could not read the chain.
private struct PresenceReadFailure: Error {}

// MARK: - Support

private extension AwaitInputsTests {
    /// Defaults bound the pathological case only. Every wait here ends on a look or on the stream, so a
    /// generous budget changes how long a test may take and never what it concludes.
    func timing(
        holdOut: Duration = .seconds(60),
        idleLimit: Duration = .seconds(60)
    ) -> InputWaitTiming {
        InputWaitTiming(holdOut: holdOut, idleLimit: idleLimit, dateProvider: StubDateProvider(now))
    }

    /// Emits each look in order. `settling` keeps the subscription open afterwards, the way a live one
    /// behaves — a stream that finishes would end the wait for a reason the chain never gave.
    func stream(_ looks: [Set<Int>], settling: Bool = false) -> AnyAsyncSequence<Set<Int>> {
        AsyncStream<Set<Int>> { continuation in
            for look in looks {
                continuation.yield(look)
            }
            if !settling {
                continuation.finish()
            }
        }
        .eraseToAnyAsyncSequence()
    }
}
