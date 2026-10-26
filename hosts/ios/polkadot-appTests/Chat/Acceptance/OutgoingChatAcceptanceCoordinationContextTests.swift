import Foundation
import os
import Testing
@testable import polkadot_app

@Suite(.timeLimit(.minutes(10)))
struct OutgoingChatAcceptanceCoordinationContextTests {
    private let fixture = ChatAcceptanceFixture()
    private let deliveries = RecordingDeliveries()

    @Test func deliversEachAcceptanceOnceAcrossSnapshots() async {
        let context = OutgoingChatAcceptanceCoordinationContext(logger: MockLogger())
        await context.update(contacts: [fixture.makeContact()]) { Task {} }

        await process(fixture.makeMessage(), in: context)
        await process(fixture.makeMessage(), in: context)
        await deliveries.waitForDeliveries(1)

        #expect(deliveries.delivered == ["acceptance"])
    }

    @Test func skipsAcceptanceUntilItsContactIsKnown() async {
        let context = OutgoingChatAcceptanceCoordinationContext(logger: MockLogger())
        await context.update(contacts: []) { Task {} }

        await process(fixture.makeMessage(), in: context)
        await context.update(contacts: [fixture.makeContact()]) { Task {} }
        await process(fixture.makeMessage(), in: context)
        await deliveries.waitForDeliveries(1)

        #expect(deliveries.delivered == ["acceptance"])
    }

    @Test func resubscribesOnlyWhenTheContactSetChanges() async {
        let context = OutgoingChatAcceptanceCoordinationContext(logger: MockLogger())
        let subscriptions = OSAllocatedUnfairLock(initialState: 0)
        let builder = { () -> Task<Void, Never> in
            subscriptions.withLock { $0 += 1 }
            return Task {}
        }

        await context.update(contacts: [fixture.makeContact()], outgoingAcceptanceTaskBuilder: builder)
        await context.update(contacts: [fixture.makeContact()], outgoingAcceptanceTaskBuilder: builder)
        await context.update(contacts: [], outgoingAcceptanceTaskBuilder: builder)

        #expect(subscriptions.withLock { $0 } == 2)
    }
}

private extension OutgoingChatAcceptanceCoordinationContextTests {
    func process(_ message: Chat.LocalMessage, in context: OutgoingChatAcceptanceCoordinationContext) async {
        await context.process(acceptanceMessages: [message]) { [deliveries] input in
            deliveries.record(input.statementId)
        }
    }
}

private final class RecordingDeliveries: @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<[String]>(initialState: [])

    var delivered: [String] { state.withLock { $0 } }

    func record(_ id: String) {
        state.withLock { $0.append(id) }
    }

    func waitForDeliveries(_ count: Int) async {
        while state.withLock({ $0.count < count }) {
            await Task.yield()
        }
    }
}
