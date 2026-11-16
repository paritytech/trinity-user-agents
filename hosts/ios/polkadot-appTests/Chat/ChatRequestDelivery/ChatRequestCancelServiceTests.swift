@testable import polkadot_app
import BackgroundExecution
import Clocks
import CoreData
import Foundation
import MessageExchangeKit
import StatementStore
import StructuredConcurrencyTestSupport
import SubstrateSdk
import Testing

struct ChatRequestCancelServiceTests {
    private let requestId = "request"
    private let peerAccountId = Data(repeating: 9, count: 32)
    private let identityKeyId = "//wallet//main"
    private let timestampPriority: UInt64 = 0xFFFF_FFFF_0000_0100
    private let currentPeriod: UInt32 = 100
    private let loader = StubPendingOutgoingChatRequestLoader(pending: nil)
    private let delivery = RecordingOutgoingChatRequestDelivery()
    private let builder = RecordingChatRequestStatementBuilder()
    private let signers = RecordingStatementSignerManager()
    private let store = MockChatRequestStoreService()
    private let cleanup = SpyChatRemovalCleanupService()
    private let periodProvider = StubNotificationAllocator()

    init() {
        periodProvider.period = currentPeriod
    }

    @Test func publishesOneCancelPerLiveCopyWithItsOwnKey() async throws {
        let currentKeyId = try ChatKeyPaths.request(requestId: requestId, period: currentPeriod)
        let previousKeyId = try ChatKeyPaths.request(requestId: requestId, period: currentPeriod - 1)
        let submitter = try RecordingStatementStoreConnection(stored: [
            storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1),
            storedCopy(signedBy: currentKeyId, expiry: timestampPriority + 2),
            storedCopy(signedBy: previousKeyId, expiry: timestampPriority + 3)
        ])

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(signers.requestedKeyIds == [identityKeyId, currentKeyId, previousKeyId])
        #expect(submitter.submitCount == 3)
        #expect(submitter.submittedSenders == [
            RecordingStatementSignerManager.accountId(for: identityKeyId),
            RecordingStatementSignerManager.accountId(for: currentKeyId),
            RecordingStatementSignerManager.accountId(for: previousKeyId)
        ])
        #expect(builder.builtExpiries == [timestampPriority + 2, timestampPriority + 3, timestampPriority + 4])
        #expect(store.calls == [.markCancelled(requestId: requestId)])
    }

    @Test func copyFromUnknownAccountIsSkipped() async throws {
        let stranger = try storedCopy(signedBy: "stranger", expiry: timestampPriority + 5)
        let submitter = RecordingStatementStoreConnection(stored: [stranger])

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 0)
    }

    @Test func marksCancelledAndCleansUpBeforePublishing() async throws {
        let executor = CountingCancelExecutor()
        executor.onExecute = { [store, cleanup, peerAccountId, requestId] in
            #expect(store.calls == [.markCancelled(requestId: requestId)])
            #expect(cleanup.accountIds == [peerAccountId])
        }

        try await makeService(pending: makePending(), executor: executor).cancel(requestId: requestId)

        #expect(executor.executeCount == 1)
    }

    @Test func failedCopyIsLoggedAndOtherCopiesAreStillCancelled() async throws {
        let identityCopy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let currentKeyId = try ChatKeyPaths.request(requestId: requestId, period: currentPeriod)
        let currentCopy = try storedCopy(signedBy: currentKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(
            failures: [StatementSubmitError.rejected(.accountFull)],
            stored: [identityCopy, currentCopy]
        )

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 2)
        #expect(store.calls == [.markCancelled(requestId: requestId)])
    }

    @Test func channelPriorityTooLowIsRetriedWithFreshCancel() async throws {
        let copy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(
            failures: [StatementSubmitError.rejected(.channelPriorityTooLow)],
            stored: [copy]
        )

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 2)
        #expect(builder.builtExpiries.count == 2)
    }

    @Test func connectionFailuresAreRetried() async throws {
        let copy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(
            failures: [TestSubmitFailure(), JSONRPCOperationError.timeout],
            stored: [copy]
        )

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 3)
    }

    @Test func storeRejectionIsNotRetried() async throws {
        let copy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(
            failures: [StatementSubmitError.rejected(.noAllowance)],
            stored: [copy]
        )

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 1)
    }

    @Test func failingCopyStopsAfterTenAttempts() async throws {
        let copy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(
            failures: Array(repeating: TestSubmitFailure(), count: 10),
            stored: [copy]
        )

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 10)
        #expect(store.calls == [.markCancelled(requestId: requestId)])
    }

    @Test func failedFetchIsRetriedThenCopiesAreCancelled() async throws {
        let copy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(stored: [copy], fetchFailures: [TestSubmitFailure()])

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.fetchCount == 2)
        #expect(submitter.submitCount == 1)
    }

    @Test func fetchFailingEveryAttemptLeavesRequestCancelled() async throws {
        let submitter = RecordingStatementStoreConnection(
            fetchFailures: Array(repeating: TestSubmitFailure(), count: 10)
        )

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.fetchCount == 10)
        #expect(submitter.submitCount == 0)
        #expect(store.calls == [.markCancelled(requestId: requestId)])
        #expect(cleanup.accountIds == [peerAccountId])
    }

    @Test(.timeLimit(.minutes(5))) func retryWaitsDoubleFromOneSecond() async throws {
        let copy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(
            failures: [TestSubmitFailure(), TestSubmitFailure()],
            stored: [copy]
        )
        let clock = ManualClock()
        let service = makeService(pending: makePending(), submitter: submitter, clock: clock)
        let cancellation = Task { try await service.cancel(requestId: requestId) }

        await clock.resumeSleep(for: .seconds(1))
        await clock.resumeSleep(for: .seconds(2))
        try await cancellation.value

        #expect(submitter.submitCount == 3)
    }

    @Test func emptyFetchPublishesNothing() async throws {
        let submitter = RecordingStatementStoreConnection()

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 0)
        #expect(delivery.retired == [requestId])
    }

    @Test func cancelExpiryExceedsOwnCopyOnChannel() async throws {
        let oneShotKeyId = try ChatKeyPaths.request(requestId: requestId, period: currentPeriod)
        let stored = try storedCopy(signedBy: oneShotKeyId, expiry: timestampPriority + 10)
        let submitter = RecordingStatementStoreConnection(stored: [stored])

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(builder.builtExpiries == [timestampPriority + 11])
    }

    @Test func cancelExpiryUsesTimestampWhenCopyExpiryIsOlder() async throws {
        let oneShotKeyId = try ChatKeyPaths.request(requestId: requestId, period: currentPeriod)
        let stored = try storedCopy(signedBy: oneShotKeyId, expiry: timestampPriority - 10)
        let submitter = RecordingStatementStoreConnection(stored: [stored])

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(builder.builtExpiries == [timestampPriority])
    }

    @Test func identityCopyIsCancelledWithIdentityKeyAndExpiry() async throws {
        let stored = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 7)
        let submitter = RecordingStatementStoreConnection(stored: [stored])

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(builder.builtExpiries == [timestampPriority + 8])
        #expect(submitter.fetchCount == 1)
    }

    @Test func fetchesCopiesAfterRetireSoPublishDuringWaitIsCancelled() async throws {
        let copy = try storedCopy(signedBy: identityKeyId, expiry: timestampPriority + 1)
        let submitter = RecordingStatementStoreConnection(stored: [copy])
        delivery.onRetire = { #expect(submitter.fetchCount == 0) }

        try await makeService(pending: makePending(), submitter: submitter).cancel(requestId: requestId)

        #expect(submitter.submitCount == 1)
    }

    @Test func cancelProofBindsCancelledRequestId() async throws {
        try await makeService(pending: makePending()).cancel(requestId: requestId)

        #expect(store.cancelProofRequestIds == [requestId])
    }

    @Test func notPendingThrowsWithoutTouchingStoreOrPublishing() async throws {
        let submitter = RecordingStatementStoreConnection()
        let service = makeService(pending: nil, submitter: submitter)

        await #expect(throws: ChatRequestCancelError.self) {
            try await service.cancel(requestId: requestId)
        }

        #expect(store.calls.isEmpty)
        #expect(delivery.retired.isEmpty)
        #expect(submitter.fetchCount == 0)
    }

    @Test func markNotPendingThrowsWithoutCleanupOrPublishing() async throws {
        let submitter = RecordingStatementStoreConnection()
        let failingStore = MockChatRequestStoreService(markCancelledError: ChatRequestCancelError.notPending)
        let service = makeService(pending: makePending(), submitter: submitter, store: failingStore)

        await #expect(throws: ChatRequestCancelError.self) {
            try await service.cancel(requestId: requestId)
        }

        #expect(cleanup.accountIds.isEmpty)
        #expect(delivery.retired.isEmpty)
        #expect(submitter.fetchCount == 0)
    }
}

private extension ChatRequestCancelServiceTests {
    func makeService(
        pending: OutgoingChatRequestInput?,
        submitter: RecordingStatementStoreConnection = RecordingStatementStoreConnection(),
        executor: BackgroundExecuting = InlineBackgroundExecutor(),
        store: MockChatRequestStoreService? = nil,
        clock: any Clock<Duration> = ImmediateClock()
    ) -> ChatRequestCancelService {
        loader.setPending(pending)

        return ChatRequestCancelService(
            dependencies: ChatRequestCancelDependencies(
                loader: loader,
                delivery: delivery,
                builder: builder,
                signerManager: signers,
                periodProvider: periodProvider,
                connection: submitter,
                priorityFactory: FixedStatementPriorityFactory(timestampPriority: timestampPriority),
                removalCleanup: cleanup,
                requestStore: store ?? self.store,
                backgroundExecutor: executor,
                logger: StubLogger(),
                clock: clock
            )
        )
    }

    func storedCopy(signedBy keyId: String, expiry: UInt64) throws -> Data {
        try UnsignedStatement()
            .addExpiry(expiry)
            .signed(by: StubStatementSigner(accountId: RecordingStatementSignerManager.accountId(for: keyId)))
            .build()
            .encodedStatement
    }

    func makePending() -> OutgoingChatRequestInput {
        OutgoingChatRequestInput(
            message: Chat.RequestMessage(
                messageId: requestId,
                timestamp: 1,
                content: .v1(Chat.RequestContentV1(pushToken: nil, welcomeMessage: nil))
            ),
            session: MessageExchange.SessionRequest(
                own: .init(signKeyId: identityKeyId, encryptionKeyId: "main", pin: nil),
                peer: .init(
                    accountId: peerAccountId,
                    publicKey: Data(repeating: 1, count: 32),
                    pin: nil,
                    devices: []
                )
            )
        )
    }
}

struct ChatRequestOutgoingCancelWriteTests {
    private let facade = UserDataStorageTestFacade()
    private let requestId = "request"
    private let peerAccountId = Data(repeating: 9, count: 32)

    @Test func keepsRequestAsCancelledAndDeletesContactChatMessageAndPublication() async throws {
        try await insertOutgoingRequest()

        let cancelled = try await facade.databaseService.performWrite { [requestId] context in
            try context.cancelOutgoingChatRequest(requestId)
        }

        let remaining = try await facade.databaseService.performRead { [requestId, peerAccountId] context in
            let contact: CDChatContact? = try context.first(for: .contact(for: peerAccountId))
            let chat: CDChat? = try context.first(for: .chatWithContact(for: peerAccountId))
            let messages = try context.fetch(CDChatMessage.fetchRequest())
            let request = try context.fetch(CDChatRequest.fetchRequest()).first

            return try RemainingRows(
                hasContact: contact != nil,
                hasChat: chat != nil,
                hasMessage: !messages.isEmpty,
                requestStatus: request.map(\.status),
                period: context.outgoingStatementPeriod(for: requestId)
            )
        }

        #expect(cancelled)
        #expect(remaining == RemainingRows(
            hasContact: false,
            hasChat: false,
            hasMessage: false,
            requestStatus: Chat.RequestStatus.outgoing(.cancelled).rawValue,
            period: nil
        ))
    }

    @Test func secondCancelOfTheSameRequestIsRejected() async throws {
        try await insertOutgoingRequest()

        let results = try await facade.databaseService.performWrite { [requestId] context in
            try [context.cancelOutgoingChatRequest(requestId), context.cancelOutgoingChatRequest(requestId)]
        }

        #expect(results == [true, false])
    }

    @Test func recordsRemovedChatInTheSameWrite() async throws {
        try await insertOutgoingRequest()

        _ = try await facade.databaseService.performWrite { [requestId] context in
            try context.cancelOutgoingChatRequest(requestId)
        }

        let removedChatIdentifier = try await facade.databaseService.performRead { [peerAccountId] context in
            let removedChat: CDRemovedChat? = try context.first(for: .removedChat(for: peerAccountId))
            return removedChat?.identifier
        }

        #expect(removedChatIdentifier == peerAccountId.toHex())
    }
}

private extension ChatRequestOutgoingCancelWriteTests {
    func insertOutgoingRequest() async throws {
        try await facade.databaseService.performWrite { [requestId, peerAccountId] context in
            let contact = CDChatContact(context: context)
            contact.identifier = peerAccountId.toHex()
            contact.username = "peer"
            contact.publicKey = Data(repeating: 1, count: 32)

            let chat = CDChat(context: context)
            chat.identifier = Chat.Id.person(peerAccountId).rawRepresentation

            let message = CDChatMessage(context: context)
            message.messageId = requestId
            message.status = Chat.LocalMessage.Status.outgoing(.new).rawValue
            message.chat = chat

            let request = CDChatRequest(context: context)
            request.identifier = requestId
            request.status = Chat.RequestStatus.outgoing(.pending).rawValue
            request.contactId = peerAccountId.toHex()
            request.message = message
            request.contact = contact
            contact.chatRequest = request

            try context.setOutgoingStatementPeriod(20_000, for: requestId)
        }
    }
}

private struct RemainingRows: Equatable {
    let hasContact: Bool
    let hasChat: Bool
    let hasMessage: Bool
    let requestStatus: Int16?
    let period: UInt32?
}

private struct TestSubmitFailure: Error {}

private final class CountingCancelExecutor: BackgroundExecuting, @unchecked Sendable {
    private let lock = NSLock()
    private var recordedExecuteCount = 0

    var onExecute: (() -> Void)?

    var executeCount: Int {
        lock.withLock { recordedExecuteCount }
    }

    func execute<T: Sendable>(_ operation: @escaping @Sendable () async throws -> T) async throws -> T {
        lock.withLock { recordedExecuteCount += 1 }
        onExecute?()
        return try await operation()
    }
}
