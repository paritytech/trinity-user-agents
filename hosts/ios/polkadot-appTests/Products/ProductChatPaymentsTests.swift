import Coinage
import DurableTransactions
import Foundation
import os
import SubstrateSdk
import Testing
import TrUAPIHost
@testable import polkadot_app

/// Payments in the rooms of the chat product take the native coinage flow: the handoff of the coins commits with the
/// host's record of the payment, the record goes to the product as `SendPayment` under the same message id until the
/// product has it, and the monitor claims and follows the coins by message id as it does for a native message.
@Suite("Chat product payments", .serialized)
struct ProductChatPaymentsTests {
    private let room = ProductPaymentRoom(productId: "chat.dot", roomId: "dm-7")
    private let keys = [Data(repeating: 0x07, count: 64), Data(repeating: 0x08, count: 64)]
    private let context = DenominationBreakdownContext(unit: 1_000, precision: 3, maxExponent: 10, minExponent: 0)

    // MARK: - Sending

    /// The handoff commits inside the transaction that keeps the payment, and the product gets the same id, time,
    /// amount and keys.
    @Test func aPaymentCommitsWithItsRecordAndGoesToTheProduct() async throws {
        let world = PaymentWorld()
        var committedInside = false

        try await world.submitter(room: room).sendTransfer(
            TransferMemo(entries: keys, totalValue: 10),
            to: Data(repeating: 0xB0, count: 32),
            messageId: "pay-1"
        ) { scope in
            guard let context = (scope as? CoreDataRegistrationScope)?.context else { return }
            committedInside = try context.count(for: CDProductChatPayment.fetchRequest()) == 1
        }
        await world.payments.waitUntilIdle()

        #expect(committedInside)
        let sent = ProductChatUserActionRequest(
            roomId: "dm-7",
            action: .sendPayment(messageId: "pay-1", timestamp: 42, totalValue: "10", coinKeys: keys)
        )
        #expect(world.querying.actionRequests == [sent])
        let stored = try await world.store.payments()
        #expect(stored.map(\.messageId) == ["pay-1"])
        #expect(stored.first?.delivery == .delivered)
        #expect(stored.first?.coinKeys == keys)
    }

    /// A recipient without a room of the chat product has nowhere to take the keys: nothing commits.
    @Test func aPaymentToAnAccountWithoutARoomCommitsNothing() async throws {
        let world = PaymentWorld()
        var committed = false

        await #expect(throws: TransferSubmitterError.noChatContact) {
            try await world.submitter(room: nil).sendTransfer(
                TransferMemo(entries: keys, totalValue: 10),
                to: Data(repeating: 0xB0, count: 32),
                messageId: "pay-1"
            ) { _ in committed = true }
        }

        #expect(!committed)
        #expect(try await world.store.payments().isEmpty)
        #expect(world.querying.actionRequests.isEmpty)
    }

    /// The wallet pays a contact through the room of the contact.
    @Test func aPaymentFromTheWalletGoesToTheRoomOfTheContact() async throws {
        let world = PaymentWorld()
        let recipient = Data(repeating: 0xB0, count: 32)
        let room = room

        try await world.submitter(room: nil, contactRoom: { $0 == recipient ? room : nil }).sendTransfer(
            TransferMemo(entries: keys, totalValue: 10),
            to: recipient,
            messageId: "pay-1"
        ) { _ in }
        await world.payments.waitUntilIdle()

        #expect(world.querying.actionRequests.map(\.roomId) == ["dm-7"])
    }

    /// A commit that throws keeps no payment, so nothing goes to the product.
    @Test func aCommitThatThrowsKeepsNoPayment() async throws {
        let world = PaymentWorld()

        await #expect(throws: PaymentTestError.commitFailed) {
            try await world.submitter(room: room).sendTransfer(
                TransferMemo(entries: keys, totalValue: 10),
                to: Data(repeating: 0xB0, count: 32),
                messageId: "pay-1"
            ) { _ in throw PaymentTestError.commitFailed }
        }
        await world.payments.waitUntilIdle()

        #expect(try await world.store.payments().isEmpty)
        #expect(world.querying.actionRequests.isEmpty)
    }

    /// A payment that did not go goes again under the same id, which the product answers with the same message.
    @Test func aPaymentThatDidNotGoGoesAgainUnderTheSameId() async throws {
        let failing = OSAllocatedUnfairLock(initialState: true)
        let world = PaymentWorld { _ in
            if failing.withLock({ $0 }) {
                failing.withLock { $0 = false }
                throw ChatQueryError.noAnswer
            }
            return .acknowledged
        }

        try await world.submitter(room: room).sendTransfer(
            TransferMemo(entries: keys, totalValue: 10),
            to: Data(repeating: 0xB0, count: 32),
            messageId: "pay-1"
        ) { _ in }
        await waitUntil { try await world.store.payments().first?.delivery == .delivered }

        #expect(world.querying.actionRequests.map(\.action.paymentId) == ["pay-1", "pay-1"])
    }

    /// A payment that waits from before the app started goes when the app starts.
    @Test func aWaitingPaymentGoesWhenTheAppStarts() async throws {
        let world = PaymentWorld()
        try await world.store.saveOutgoing(payment("pay-1", incoming: false, delivery: .sending)) { _ in }

        await world.payments.resume()
        await world.payments.waitUntilIdle()

        #expect(world.querying.actionRequests.map(\.action.paymentId) == ["pay-1"])
        #expect(try await world.store.payments().first?.delivery == .delivered)
    }

    /// The product refused the payment: it does not go again, as a native message that failed for good, and the
    /// monitor leaves it.
    @Test func aRefusedPaymentDoesNotGoAgain() async throws {
        let world = PaymentWorld { _ in throw ChatQueryError.actionRefused(.rejected(reason: "No contact")) }

        try await world.submitter(room: room).sendTransfer(
            TransferMemo(entries: keys, totalValue: 10),
            to: Data(repeating: 0xB0, count: 32),
            messageId: "pay-1"
        ) { _ in }
        await world.payments.waitUntilIdle()

        #expect(world.querying.actionRequests.count == 1)
        #expect(try await world.store.payments().first?.delivery == .refused)
        let unsettled = await world.payments.unsettledPayments()
        for await payments in unsettled {
            #expect(payments.isEmpty)
            break
        }
    }

    // MARK: - Receiving

    /// The product can hand the same payment over again: the host keeps it once and accepts each time.
    @Test func aReceivedPaymentIsKeptOnce() async throws {
        let world = PaymentWorld()

        let first = try await world.payments.receive(productId: "chat.dot", payment: received("in-1"))
        let second = try await world.payments.receive(productId: "chat.dot", payment: received("in-1"))

        #expect(first == .accepted)
        #expect(second == .accepted)
        let stored = try await world.store.payments()
        #expect(stored.map(\.messageId) == ["in-1"])
        #expect(stored.first?.incoming == true)
        #expect(stored.first?.roomId == "dm-7")
        #expect(stored.first?.coinKeys == keys)
    }

    /// A payment that comes before the transfer monitor runs waits for it: the monitor reads it when it starts.
    @Test func aPaymentBeforeTheMonitorWaitsForIt() async throws {
        let world = PaymentWorld()

        let receipt = try await world.payments.receive(productId: "chat.dot", payment: received("in-1"))

        #expect(receipt == .accepted)
        var waiting = await world.payments.unsettledPayments().makeAsyncIterator()
        #expect(await waiting.next()?.map(\.messageId) == ["in-1"])
    }

    // MARK: - Monitor

    /// The monitor claims a received payment under its message id, in the native window, and the rooms see each
    /// state.
    @Test func theMonitorClaimsAReceivedPaymentByItsMessageId() async throws {
        let world = PaymentWorld()
        let events = await world.payments.events()
        let claims = MockClaimCoinsService(detections: [
            .detecting, .claiming, .claimed(amount: 10, finalized: true)
        ])
        let monitor = world.monitor(claims: claims, context: context)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        _ = try await world.payments.receive(productId: "chat.dot", payment: received("in-1"))
        let states = await collectStates(of: "in-1", from: events) {
            $0 == .incoming(.init(status: .claimed, actualValue: 10))
        }

        #expect(states.last == .incoming(.init(status: .claimed, actualValue: 10)))
        #expect(states.contains(.incoming(.init(status: .claiming))))
        #expect(claims.calls.map(\.groupId) == ["in-1"])
        #expect(claims.calls.first?.coinKeys == keys)
        let stored = try await world.store.payments().first
        #expect(stored?.state == .incoming(.init(status: .claimed, actualValue: 10)))
        let window = try #require(claims.calls.first?.retryUntil.timeIntervalSinceNow)
        #expect(abs(window - CoinageConstants.claimRetryWindow) < 60)
    }

    /// The monitor follows the coins of a payment of the user until the contact claimed them.
    @Test func theMonitorFollowsAPaymentOfTheUser() async throws {
        let world = PaymentWorld()
        let events = await world.payments.events()
        let coin = try makeCoin(exponent: 2)
        let statuses = MockCoinageTransferStatusService(snapshots: [
            [:],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .awaitingClaim)],
            [coin.publicKey: CoinageTransferState(coin: coin, status: .claimed(finalized: true))]
        ])
        let monitor = world.monitor(statuses: statuses, context: context)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }

        try await world.submitter(room: room).sendTransfer(
            TransferMemo(entries: keys, totalValue: 10),
            to: Data(repeating: 0xB0, count: 32),
            messageId: "pay-1"
        ) { _ in }
        let claimed = OutgoingTransferState(status: .claimed, actualValue: context.valueInPlanks(for: 2))
        let states = await collectStates(of: "pay-1", from: events) { $0 == .outgoing(claimed) }

        #expect(states == [.outgoing(.init(status: .sending)), .outgoing(.init(status: .sent)), .outgoing(claimed)])
        #expect(try await world.store.payments().first?.state == .outgoing(claimed))
    }

    // MARK: - Room

    /// A payment of the user shows at the end of the room at once, with the native state of its transfer, and the
    /// message of the product takes its place under the same id.
    @Test func aPaymentShowsInTheRoomWithTheStateOfItsTransfer() async throws {
        let release = OSAllocatedUnfairLock(initialState: false)
        let world = PaymentWorld { request in
            while !release.withLock({ $0 }) {
                try await Task.sleep(for: .milliseconds(2))
            }
            guard case let .sendPayment(messageId, timestamp, totalValue, _) = request.action else {
                return .acknowledged
            }
            return .messageCreated(ChatMessage(
                messageId: messageId,
                origin: ChatMessageOrigin(participantId: productUserParticipantId),
                timestamp: timestamp,
                body: .payment(ChatPaymentRecord(amount: totalValue, legacy: false)),
                status: .sent,
                replyTo: nil,
                replyPreview: nil,
                editedAt: nil,
                reactions: [],
                request: false
            ))
        }
        let source = world.source(room: room)
        await source.start()
        await source.waitForFirstPage()

        try await world.submitter(room: room).sendTransfer(
            TransferMemo(entries: keys, totalValue: 10),
            to: Data(repeating: 0xB0, count: 32),
            messageId: "pay-1"
        ) { _ in }
        await waitUntil { await source.snapshot.messages.contains { $0.messageId == "pay-1" } }

        let waiting = try #require(await source.snapshot.messages.first { $0.messageId == "pay-1" })
        #expect(waiting.status == .outgoing(.new))
        #expect(waiting.content == .coinageSend(.init(totalValue: 10, coinKeys: [])))

        release.withLock { $0 = true }
        await world.payments.waitUntilIdle()
        try await world.payments.updateOutgoing(messageId: "pay-1", state: .init(status: .sent))
        await waitUntil {
            await source.snapshot.messages.first { $0.messageId == "pay-1" }?.content
                == .coinageSend(.init(totalValue: 10, coinKeys: [], state: .outgoing(.init(status: .sent))))
        }

        let sent = await source.snapshot.messages.filter { $0.messageId == "pay-1" }
        #expect(sent.count == 1)
        #expect(sent.first?.status == .outgoing(.sent))
        #expect(sent.first?.content == .coinageSend(.init(
            totalValue: 10,
            coinKeys: [],
            state: .outgoing(.init(status: .sent))
        )))
    }

    /// A payment of the contact takes the state of its claim, which the host keeps by message id.
    @Test func aPaymentOfTheContactShowsTheStateOfItsClaim() {
        let message = ChatMessage(
            messageId: "in-1",
            origin: ChatMessageOrigin(participantId: "contact"),
            timestamp: 1,
            body: .payment(ChatPaymentRecord(amount: "10", legacy: false)),
            status: .read,
            replyTo: nil,
            replyPreview: nil,
            editedAt: nil,
            reactions: [],
            request: false
        )
        let state = Chat.LocalMessage.Content.Transfer.State.incoming(.init(status: .claiming))

        let local = message.localMessage(chatId: .chatExtension("chat.dot", roomId: "dm-7"), extensionId: "chat.dot")
        let claiming = message.localMessage(
            chatId: .chatExtension("chat.dot", roomId: "dm-7"),
            extensionId: "chat.dot",
            transferState: state
        )

        #expect(local.content == .coinageSend(.init(totalValue: 10, coinKeys: [])))
        #expect(claiming.content == .coinageSend(.init(totalValue: 10, coinKeys: [], state: state)))
    }
}

// MARK: - World

private enum PaymentTestError: Error {
    case commitFailed
}

private final class PaymentWorld {
    let facade = UserDataStorageTestFacade()
    let store: ProductChatPaymentStore
    let querying: ScriptedProductChatQuerying
    let payments: ProductChatPayments

    init(
        action: @escaping ScriptedProductChatQuerying.ActionAnswer = { _ in .acknowledged }
    ) {
        store = ProductChatPaymentStore(storageFacade: facade)
        querying = ScriptedProductChatQuerying(action: action)
        payments = ProductChatPayments(
            store: store,
            delivery: ProductQueryingDelivery(querying: querying),
            worker: NoProductHopUploads(),
            settings: .init(firstRetryDelay: .milliseconds(5), maxRetryDelay: .milliseconds(20)),
            logger: StubLogger()
        )
    }

    func submitter(
        room: ProductPaymentRoom?,
        contactRoom: @escaping @Sendable (AccountId) async throws -> ProductPaymentRoom? = { _ in nil }
    ) -> ProductChatSubmitter {
        ProductChatSubmitter(room: room, contactRoom: contactRoom, payments: payments, now: { 42 })
    }

    func monitor(
        claims: MockClaimCoinsService = MockClaimCoinsService(detections: []),
        statuses: MockCoinageTransferStatusService = MockCoinageTransferStatusService(snapshots: []),
        context: DenominationBreakdownContext
    ) -> CoinageTransferMonitor {
        CoinageTransferMonitor(
            claimCoinsService: claims,
            transferStatusService: statuses,
            denominationContext: { context },
            transferStateStore: TransferStateCoreDataStore(storageFacade: facade),
            productPayments: payments,
            storageFacade: facade,
            operationQueue: OperationQueue(),
            logger: MockLogger()
        )
    }

    func source(room: ProductPaymentRoom) -> ProductMessageSource {
        ProductMessageSource(
            chatId: .chatExtension(room.productId, roomId: room.roomId),
            productId: room.productId,
            roomId: room.roomId,
            querying: querying,
            cache: ProductMessageCache(),
            settings: .init(retryDelay: .milliseconds(1)),
            now: { 1_000_000 },
            outbox: .inMemory(querying: querying, logger: StubLogger()),
            payments: payments,
            logger: StubLogger()
        )
    }
}

private extension ProductChatPaymentsTests {
    func payment(_ messageId: String, incoming: Bool, delivery: ProductChatPayment.Delivery) -> ProductChatPayment {
        ProductChatPayment(
            messageId: messageId,
            productId: room.productId,
            roomId: room.roomId,
            incoming: incoming,
            timestamp: 1,
            totalValue: 10,
            coinKeys: keys,
            delivery: delivery,
            state: nil
        )
    }

    func received(_ messageId: String) -> HostPersonChatPaymentReceivedRequest {
        HostPersonChatPaymentReceivedRequest(
            roomId: room.roomId,
            messageId: messageId,
            timestamp: 1,
            totalValue: "10",
            coinKeys: keys
        )
    }

    /// The states of a payment that the rooms see, up to and including the first one satisfying `until`.
    func collectStates(
        of messageId: String,
        from events: AsyncStream<ProductChatPayments.Event>,
        until: (Chat.LocalMessage.Content.Transfer.State) -> Bool
    ) async -> [Chat.LocalMessage.Content.Transfer.State] {
        var states: [Chat.LocalMessage.Content.Transfer.State] = []
        for await event in events {
            guard case let .state(id, state) = event, id == messageId else { continue }
            if states.last != state {
                states.append(state)
            }
            if until(state) { break }
        }
        return states
    }

    func makeCoin(exponent: Int16) throws -> Coin {
        let installation = try CoinageInstallationId(value: Data(repeating: 0x01, count: 32))
        return Coin(
            exponent: exponent,
            derivationIndex: CoinageKeyIndex(installation: installation, item: 0),
            age: 0,
            publicKey: Data(repeating: 0x09, count: 32)
        )
    }
}

private extension ChatUserAction {
    var paymentId: String? {
        guard case let .sendPayment(messageId, _, _, _) = self else { return nil }
        return messageId
    }
}

private func waitUntil(_ condition: @Sendable () async throws -> Bool) async {
    for _ in 0 ..< 1_000 where await (try? condition()) != true {
        try? await Task.sleep(for: .milliseconds(2))
    }
}
