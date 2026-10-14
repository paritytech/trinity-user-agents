import Foundation
import Testing
@testable import polkadot_app

/// The transfer state row is the bubble's only source of truth, so it has to be written once per real
/// change and never for a repeat: a repeat that dirtied the row would refresh the whole message list,
/// and a change that did not touch the message would never reach it.
@Suite("Transfer state store")
struct TransferStateStoreTests {
    // MARK: - First-attempt anchor

    @Test("the anchor is written once and reused on every later call")
    func anchorIsStable() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)

        let first = try await world.store.beginIncoming(messageId: message.messageId)
        let second = try await world.store.beginIncoming(messageId: message.messageId)

        #expect(first == second)
    }

    @Test("a later call does not move the anchor even as the clock advances")
    func anchorDoesNotFollowTheClock() async throws {
        let start = Date(timeIntervalSince1970: 1_700_000_000)
        let dateProvider = MockDateProvider(now: start)
        let world = TransferStateTestWorld(dateProvider: dateProvider)
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)

        let first = try await world.store.beginIncoming(messageId: message.messageId)
        await dateProvider.advance(by: 60)
        let second = try await world.store.beginIncoming(messageId: message.messageId)

        #expect(first == start)
        #expect(second == start)
    }

    @Test("each message gets its own anchor")
    func anchorsArePerMessage() async throws {
        let dateProvider = MockDateProvider()
        let world = TransferStateTestWorld(dateProvider: dateProvider)
        try await world.setup()
        let first = try await world.saveTransfer(.incoming)
        let other = try await world.saveTransfer(.incoming)

        let firstAnchor = try await world.store.beginIncoming(messageId: first.messageId)
        await dateProvider.advance(by: 60)
        let otherAnchor = try await world.store.beginIncoming(messageId: other.messageId)

        #expect(otherAnchor == firstAnchor.addingTimeInterval(60))
    }

    /// Two launches asking at once must not split the anchor: the read and the insert are one
    /// transaction, so whichever runs second sees the row the first wrote and no second row appears.
    @Test("concurrent first callers agree on one anchor")
    func concurrentCallersAgree() async throws {
        let world = TransferStateTestWorld(dateProvider: MockDateProvider())
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)

        async let first = world.store.beginIncoming(messageId: message.messageId)
        async let second = world.store.beginIncoming(messageId: message.messageId)

        let anchors = try await [first, second]
        #expect(anchors[0] == anchors[1])
        #expect(try await world.incomingRowCount() == 1)
    }

    @Test("beginning a claim leaves the message in the detecting state")
    func beginWritesDetecting() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)

        _ = try await world.store.beginIncoming(messageId: message.messageId)

        let transfer = try #require(try await world.transfer(message.messageId))
        #expect(transfer.state == .incoming(.init(status: .detecting)))
    }

    // MARK: - Updates

    @Test("an incoming update is read back through the message as its state")
    func incomingUpdateIsReadBack() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming, totalValue: 10)

        try await world.store.updateIncoming(
            messageId: message.messageId,
            state: .init(status: .claimed, actualValue: 7)
        )

        let transfer = try #require(try await world.transfer(message.messageId))
        #expect(transfer.state == .incoming(.init(status: .claimed, actualValue: 7)))
        #expect(transfer.totalValue == 10)
    }

    @Test("an outgoing update is read back through the message as its state")
    func outgoingUpdateIsReadBack() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.outgoing)

        try await world.store.updateOutgoing(messageId: message.messageId, state: .init(status: .sent))

        let transfer = try #require(try await world.transfer(message.messageId))
        #expect(transfer.state == .outgoing(.init(status: .sent)))
    }

    @Test("a message with no row has no state")
    func noRowMeansNoState() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.outgoing)

        let transfer = try #require(try await world.transfer(message.messageId))
        #expect(transfer.state == nil)
    }

    @Test("a changed state re-emits the message with the new state")
    func changedStateReemitsMessage() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)
        let states = world.stateStream(of: message.messageId)

        try await world.store.updateIncoming(messageId: message.messageId, state: .init(status: .claiming))
        try await world.store.updateIncoming(
            messageId: message.messageId,
            state: .init(status: .claimed, actualValue: 10)
        )

        let seen = try await states.collect { $0 == .incoming(.init(status: .claimed, actualValue: 10)) }
        #expect(seen.contains(.incoming(.init(status: .claiming))))
        #expect(seen.last == .incoming(.init(status: .claimed, actualValue: 10)))
    }

    @Test("writing the same state again does not re-emit the message")
    func equalUpdateIsNoop() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)
        let states = world.stateStream(of: message.messageId)

        try await world.store.updateIncoming(messageId: message.messageId, state: .init(status: .claiming))
        try await world.store.updateIncoming(messageId: message.messageId, state: .init(status: .claiming))
        try await world.store.updateIncoming(messageId: message.messageId, state: .init(status: .claiming))
        try await world.store.updateIncoming(messageId: message.messageId, state: .init(status: .failed))

        let seen = try await states.collect { $0 == .incoming(.init(status: .failed)) }
        let claimingEmissions = seen.filter { $0 == .incoming(.init(status: .claiming)) }
        #expect(claimingEmissions.count == 1)
    }

    @Test("updating a message that does not exist throws")
    func missingMessageThrows() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()

        await #expect(throws: TransferStateStoreError.self) {
            try await world.store.updateIncoming(messageId: "missing", state: .init(status: .claiming))
        }
    }

    @Test("deleting the message deletes its row")
    func deletingMessageCascades() async throws {
        let world = TransferStateTestWorld()
        try await world.setup()
        let message = try await world.saveTransfer(.incoming)
        _ = try await world.store.beginIncoming(messageId: message.messageId)
        #expect(try await world.incomingRowCount() == 1)

        try await world.deleteMessage(message.messageId)

        #expect(try await world.incomingRowCount() == 0)
    }
}
