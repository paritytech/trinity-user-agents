import Coinage
import CoreData
import Foundation
import FoundationExt
import Keystore_iOS
import SubstrateSdk
import Testing
@testable import polkadot_app

/// When the chat moves to the product, the payments of the native chat become payments of the product's rooms under
/// the same message ids: the transfer monitor keeps claiming the coins of a payment that waits for its claim and
/// keeps following one of the user, each once, and the rooms draw the states the native chat drew.
@Suite("Native chat payment adoption", .serialized)
struct NativeChatPaymentAdoptionTests {
    private let contact = Data(repeating: 0xB0, count: 32)
    private let keys = [Data(repeating: 0x07, count: 64)]
    private let context = DenominationBreakdownContext(unit: 1_000, precision: 3, maxExponent: 10, minExponent: 0)

    // MARK: - Reading the native chat

    @Test func aPaymentOfTheContactBecomesAPaymentOfItsRoomWithItsClaim() throws {
        let firstAttempt = Date(timeIntervalSince1970: 1_000)
        let claiming = Chat.LocalMessage.Content.Transfer.State.incoming(.init(status: .claiming))
        let message = message(status: .incoming(.seen), origin: .contact(contact), state: claiming)

        let adopted = try #require(
            NativeChatPaymentReader.payment(from: message, firstAttemptAt: firstAttempt, productId: "chat.dot")
        )

        #expect(adopted.firstAttemptAt == firstAttempt)
        #expect(adopted.payment == ProductChatPayment(
            messageId: "m1",
            productId: "chat.dot",
            roomId: "dm-" + String(repeating: "b0", count: 32),
            incoming: true,
            timestamp: 3_000,
            totalValue: 10,
            coinKeys: keys,
            delivery: .delivered,
            state: claiming
        ))
    }

    @Test func aPaymentOfTheUserKeepsItsStatusAndIsNeverSentAgain() throws {
        let sent = Chat.LocalMessage.Content.Transfer.State.outgoing(.init(status: .sent))
        let message = message(status: .outgoing(.delivered), origin: .user, state: sent)

        let adopted = try #require(
            NativeChatPaymentReader.payment(from: message, firstAttemptAt: nil, productId: "chat.dot")
        )

        #expect(!adopted.payment.incoming)
        #expect(adopted.payment.delivery == .delivered)
        #expect(adopted.payment.state == sent)
        #expect(adopted.firstAttemptAt == nil)
    }

    /// The native monitor followed only what this device sent and what did not fail before its coins went; the
    /// product follows the same payments, and draws the others as the native chat drew them.
    @Test func onlyThePaymentsTheNativeMonitorFollowedAreTaken() {
        let synced = message(status: .outgoing(.sent), origin: .user, state: nil, creationSource: .deviceSync)
        let failed = message(status: .outgoing(.failed), origin: .user, state: nil)
        let text = Chat.LocalMessage(
            messageId: "t1",
            chatId: .person(contact),
            origin: .user,
            creationSource: .localDevice,
            status: .outgoing(.sent),
            timestamp: 1,
            content: .text("hi"),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
        let failedAfterItsCoinsWent = message(
            status: .outgoing(.failed),
            origin: .user,
            state: .outgoing(.init(status: .sent))
        )

        #expect(NativeChatPaymentReader.payment(from: synced, firstAttemptAt: nil, productId: "chat.dot") == nil)
        #expect(NativeChatPaymentReader.payment(from: failed, firstAttemptAt: nil, productId: "chat.dot") == nil)
        #expect(NativeChatPaymentReader.payment(from: text, firstAttemptAt: nil, productId: "chat.dot") == nil)
        #expect(
            NativeChatPaymentReader.payment(from: failedAfterItsCoinsWent, firstAttemptAt: nil, productId: "chat.dot")
                != nil
        )
    }

    /// The reader takes the payment messages of the native store with the state and the first claim attempt that
    /// the native monitor wrote.
    @Test func theReaderTakesTheStateTheNativeMonitorWrote() async throws {
        let facade = UserDataStorageTestFacade()
        let chat = TestChatManager(peer: MockChatPeer.person(accountId: contact), facade: facade)
        try await chat.setup()
        let incoming = try await chat.sendMessage(
            .coinageSend(.init(totalValue: 10, coinKeys: keys)),
            status: .incoming(.new)
        )
        _ = try await chat.sendMessage(.text("hi"), status: .incoming(.new))
        let firstAttempt = Date(timeIntervalSince1970: 1_000)
        try await facade.databaseService.performWrite { context in
            let request = NSFetchRequest<CDChatMessage>(entityName: "CDChatMessage")
            request.predicate = NSPredicate(format: "%K == %@", #keyPath(CDChatMessage.messageId), incoming.messageId)
            let row = try #require(try context.fetch(request).first)
            let state = try context.insertNew(CDIncomingTransferState.self)
            state.firstAttemptAt = firstAttempt
            state.status = IncomingTransferState.Status.claiming.rawValue
            row.incomingTransferState = state
        }

        let payments = try await NativeChatPaymentReader(storageFacade: facade, logger: StubLogger())
            .payments(productId: "chat.dot")

        #expect(payments.map(\.payment.messageId) == [incoming.messageId])
        #expect(payments.first?.payment.state == .incoming(.init(status: .claiming)))
        #expect(payments.first?.firstAttemptAt == firstAttempt)
        #expect(payments.first?.payment.roomId == "dm-" + contact.toHex())
    }

    // MARK: - Keeping them

    /// Taking the payments over again changes nothing: a payment kept before keeps the state the monitor wrote since.
    @Test func adoptingAgainKeepsWhatTheHostKeeps() async throws {
        let store = ProductChatPaymentStore(storageFacade: UserDataStorageTestFacade())
        let firstAttempt = Date(timeIntervalSince1970: 1_000)
        let waiting = AdoptedChatPayment(
            payment: payment("in-1", incoming: true, state: .incoming(.init(status: .claiming))),
            firstAttemptAt: firstAttempt
        )

        #expect(try await store.adopt([waiting]).map(\.messageId) == ["in-1"])
        #expect(try await store.beginIncoming(messageId: "in-1") == firstAttempt)
        try await store.updateIncoming(messageId: "in-1", state: .init(status: .claimed, actualValue: 10))

        #expect(try await store.adopt([waiting]).isEmpty)
        let kept = try await store.payments()
        #expect(kept.count == 1)
        #expect(kept.first?.state == .incoming(.init(status: .claimed, actualValue: 10)))
    }

    /// A claim that started has a first attempt, so its window has an end even when the native row lost it.
    @Test func aStartedClaimWithoutItsFirstAttemptStartsItsWindowNow() async throws {
        let now = Date(timeIntervalSince1970: 5_000)
        let store = ProductChatPaymentStore(
            storageFacade: UserDataStorageTestFacade(),
            dateProvider: FixedDateProvider(date: now)
        )

        _ = try await store.adopt([AdoptedChatPayment(
            payment: payment("in-1", incoming: true, state: .incoming(.init(status: .detecting))),
            firstAttemptAt: nil
        )])

        #expect(try await store.beginIncoming(messageId: "in-1") == now)
    }

    // MARK: - Claiming them

    /// A payment of the contact that waits for its claim is claimed once, under its message id and in the window of
    /// its first native attempt; one that the native chat claimed is left alone, also after a second adoption.
    @Test func theMonitorClaimsWhatWaitsOnceAndLeavesWhatWasClaimed() async throws {
        let world = TransferStateTestWorld()
        let firstAttempt = Date(timeIntervalSince1970: 1_000)
        let waiting = AdoptedChatPayment(
            payment: payment("in-1", incoming: true, state: .incoming(.init(status: .claiming))),
            firstAttemptAt: firstAttempt
        )
        let claimed = AdoptedChatPayment(
            payment: payment("in-2", incoming: true, state: .incoming(.init(status: .claimed, actualValue: 10))),
            firstAttemptAt: firstAttempt
        )
        let claims = MockClaimCoinsService(detections: [.claiming, .claimed(amount: 10, finalized: true)])
        let monitor = CoinageTransferMonitor(
            claimCoinsService: claims,
            transferStatusService: MockCoinageTransferStatusService(snapshots: []),
            denominationContext: { [context] in context },
            productPayments: world.payments,
            logger: MockLogger()
        )
        let states = await world.stateStream(of: "in-1")

        #expect(try await world.payments.adopt([waiting, claimed]) == 2)
        await monitor.setup()
        defer { Task { await monitor.throttle() } }
        _ = await states.collect { $0 == .incoming(.init(status: .claimed, actualValue: 10)) }
        #expect(try await world.payments.adopt([waiting, claimed]) == 0)

        #expect(claims.calls == [MockClaimCoinsService.Call(
            coinKeys: keys,
            groupId: "in-1",
            retryUntil: firstAttempt.addingTimeInterval(CoinageConstants.claimRetryWindow)
        )])
    }

    // MARK: - Migration

    /// The payments are taken over once, on a start that knows the chat product, also on an install that staged its
    /// history before.
    @Test func thePaymentsAreTakenOverOnceWhenTheChatProductIsKnown() async throws {
        let settings = InMemorySettingsManager()
        settings.set(value: true, for: "productChatHistoryStaged")
        let reads = Reads()
        let productId = ProductIdBox()

        for id in [nil, "chat.dot", "chat.dot"] {
            productId.value = id
            let migration = ProductChatMigration(
                exporter: FailingExport(),
                deviceKey: { Data() },
                nativePayments: reads,
                adoptPayments: { $0.count },
                chatProductId: { productId.value },
                settings: settings,
                file: FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            )
            await migration.start { _ in true }
            _ = try await migration.part(offset: 0, maxLen: 1)
        }

        #expect(reads.productIds == ["chat.dot"])
    }
}

// MARK: - Helpers

private extension NativeChatPaymentAdoptionTests {
    func message(
        status: Chat.LocalMessage.Status,
        origin: Chat.LocalMessage.Origin,
        state: Chat.LocalMessage.Content.Transfer.State?,
        creationSource: Chat.LocalMessage.CreationSource = .localDevice
    ) -> Chat.LocalMessage {
        Chat.LocalMessage(
            messageId: "m1",
            chatId: .person(contact),
            origin: origin,
            creationSource: creationSource,
            status: status,
            timestamp: 3_000,
            content: .coinageSend(.init(totalValue: 10, coinKeys: keys, state: state)),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }

    func payment(
        _ messageId: String,
        incoming: Bool,
        state: Chat.LocalMessage.Content.Transfer.State?
    ) -> ProductChatPayment {
        ProductChatPayment(
            messageId: messageId,
            productId: "chat.dot",
            roomId: "dm-" + contact.toHex(),
            incoming: incoming,
            timestamp: 1,
            totalValue: 10,
            coinKeys: keys,
            delivery: .delivered,
            state: state
        )
    }
}

private final class Reads: NativeChatPaymentsReading, @unchecked Sendable {
    private let lock = NSLock()
    private var ids: [String] = []

    var productIds: [String] {
        lock.withLock { ids }
    }

    func payments(productId: String) async throws -> [AdoptedChatPayment] {
        lock.withLock { ids.append(productId) }
        return []
    }
}

private final class ProductIdBox: @unchecked Sendable {
    var value: String?
}

/// The history was staged before: an export now would be a second staging.
private struct FailingExport: NativeChatHistoryExporting {
    struct Unexpected: Error {}

    func export() async throws -> Data? {
        throw Unexpected()
    }
}

private struct FixedDateProvider: DateProviding {
    let date: Date

    func read() async -> Date {
        date
    }
}
