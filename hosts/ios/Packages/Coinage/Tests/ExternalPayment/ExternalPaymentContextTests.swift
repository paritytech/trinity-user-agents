import Foundation
import os
import Testing
@testable import Coinage

struct ExternalPaymentContextTests {
    /// Tasks that never finish on their own; completion is driven through `onComplete`.
    private final class Starts: @unchecked Sendable {
        private let started = OSAllocatedUnfairLock(initialState: [String]())

        var ids: [String] { started.withLock { $0 } }

        func execute(_ id: String) -> @Sendable () -> Task<Void, Never> {
            { [started] in
                started.withLock { $0.append(id) }
                return Task { try? await Task.sleep(for: .seconds(60)) }
            }
        }
    }

    @Test func runsOnePaymentAtATimeInFifoOrder() async {
        let context = ExternalPaymentContext()
        let starts = Starts()

        await context.scheduleIfNeeded(paymentId: "a", onExecute: starts.execute("a"))
        await context.scheduleIfNeeded(paymentId: "b", onExecute: starts.execute("b"))
        await context.scheduleIfNeeded(paymentId: "c", onExecute: starts.execute("c"))

        #expect(starts.ids == ["a"])
        #expect(await context.currentPaymentId == "a")

        await context.onComplete(paymentId: "a")
        #expect(starts.ids == ["a", "b"])

        await context.onComplete(paymentId: "b")
        #expect(starts.ids == ["a", "b", "c"])

        await context.onComplete(paymentId: "c")
        #expect(await context.currentPaymentId == nil)
    }

    @Test func duplicateSchedulesAreIgnored() async {
        let context = ExternalPaymentContext()
        let starts = Starts()

        await context.scheduleIfNeeded(paymentId: "a", onExecute: starts.execute("a"))
        await context.scheduleIfNeeded(paymentId: "a", onExecute: starts.execute("a"))
        await context.scheduleIfNeeded(paymentId: "b", onExecute: starts.execute("b"))
        await context.scheduleIfNeeded(paymentId: "b", onExecute: starts.execute("b"))
        await context.onComplete(paymentId: "a")
        await context.onComplete(paymentId: "b")

        #expect(starts.ids == ["a", "b"])
    }

    @Test func completingAForeignIdIsIgnored() async {
        let context = ExternalPaymentContext()
        let starts = Starts()

        await context.scheduleIfNeeded(paymentId: "a", onExecute: starts.execute("a"))
        await context.onComplete(paymentId: "zzz")

        #expect(await context.currentPaymentId == "a")
    }
}
