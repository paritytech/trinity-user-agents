import Foundation
import Testing

@testable import StructuredConcurrency

@Suite(.timeLimit(.minutes(5)))
struct SharedRunTests {
    private static let cancelled = -1
    private static let completed = 1

    @Test func concurrentCallersShareOneRun() async {
        let operation = HeldOperation(returning: Self.completed)
        let (run, joins) = makeRun()
        var joinEvents = joins.makeAsyncIterator()

        let first = Task { await run.run(operation.operation) }
        let second = Task { await run.run(operation.operation) }
        #expect(await joinEvents.next() == 1)
        #expect(await joinEvents.next() == 2)
        operation.release()

        #expect(await first.value == Self.completed)
        #expect(await second.value == Self.completed)
        #expect(operation.callCount == 1)
    }

    @Test func cancellingOneJoinedCallerKeepsTheRunForTheOther() async {
        let operation = HeldOperation(returning: Self.completed)
        let (run, joins) = makeRun()
        var joinEvents = joins.makeAsyncIterator()

        let expiring = Task { await run.run(operation.operation) }
        let foreground = Task { await run.run(operation.operation) }
        #expect(await joinEvents.next() == 1)
        #expect(await joinEvents.next() == 2)

        expiring.cancel()
        #expect(await expiring.value == Self.cancelled)

        operation.release()

        #expect(await foreground.value == Self.completed)
        #expect(operation.callCount == 1)
    }

    @Test func cancelledRunIsNotHandedToTheNextCaller() async {
        let operation = HeldOperation(returning: Self.completed)
        let (run, joins) = makeRun()
        var joinEvents = joins.makeAsyncIterator()

        let expiring = Task { await run.run(operation.operation) }
        #expect(await joinEvents.next() == 1)
        expiring.cancel()
        #expect(await expiring.value == Self.cancelled)

        let foreground = Task { await run.run(operation.operation) }
        #expect(await joinEvents.next() == 1)
        operation.release()

        #expect(await foreground.value == Self.completed)
    }
}

private extension SharedRunTests {
    func makeRun() -> (SharedRun<Int>, AsyncStream<Int>) {
        let (joins, joined) = AsyncStream.makeStream(of: Int.self)
        let run = SharedRun(cancelledValue: Self.cancelled, onJoin: { joined.yield($0) })
        return (run, joins)
    }
}
