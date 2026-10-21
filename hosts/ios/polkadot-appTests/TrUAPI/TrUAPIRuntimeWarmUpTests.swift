import Foundation
import os
import Testing
@testable import polkadot_app

/// The core runtime opens its database with a blocking wait, so the app builds it away from the main thread
/// (06-OPEN-ITEMS O24).
struct TrUAPIRuntimeWarmUpTests {
    private struct NotYet: Error {}

    @Test func buildsAwayFromTheMainThread() async {
        let threads = OSAllocatedUnfairLock(initialState: [Bool]())
        let warmUp = TrUAPIRuntimeWarmUp(build: {
            threads.withLock { $0.append(Thread.isMainThread) }
        })

        let built = await MainActor.run { warmUp.start() }.value

        #expect(built)
        #expect(threads.withLock { $0 } == [false])
    }

    @Test func triesAgainUntilTheBuildSucceeds() async {
        let calls = OSAllocatedUnfairLock(initialState: 0)
        let warmUp = TrUAPIRuntimeWarmUp(attempts: 5, interval: .milliseconds(10), build: {
            let call = calls.withLock { calls in
                calls += 1
                return calls
            }
            if call < 3 { throw NotYet() }
        })

        let built = await warmUp.start().value

        #expect(built)
        #expect(calls.withLock { $0 } == 3)
    }

    @Test func stopsAfterItsAttempts() async {
        let calls = OSAllocatedUnfairLock(initialState: 0)
        let warmUp = TrUAPIRuntimeWarmUp(attempts: 3, interval: .milliseconds(10), build: {
            calls.withLock { $0 += 1 }
            throw NotYet()
        })

        let built = await warmUp.start().value

        #expect(!built)
        #expect(calls.withLock { $0 } == 3)
    }
}
