import Foundation
import Testing
import Operation_iOS
import SubstrateSdk
import TrUAPIHost
import MessageExchangeKit

@testable import polkadot_app

// MARK: - Mocks

private final class InMemoryHandledRequestRepositoryFactory: SSOHandledRequestRepositoryMaking {
    let repository = InMemoryDataProviderRepository<SSOHandledRequest>()

    func createRepository() -> AnyDataProviderRepository<SSOHandledRequest> {
        AnyDataProviderRepository(repository)
    }

    func fetchAll() async throws -> [SSOHandledRequest] {
        try await repository.fetchAllOperation(with: .init()).asyncExecute()
    }

    func seed(messageId: String) async throws {
        try await repository
            .saveOperation({ [SSOHandledRequest(messageId: messageId)] }, { [] })
            .asyncExecute()
    }
}

/// Records the messages the processing context dispatches to it and lets tests
/// suspend until a target count has been handled (the context processes on a
/// detached task, so `handleMessages` returning does not imply completion).
private actor SpyRequestHandler: SSORequestHandling {
    typealias Message = SSOTrUAPIRequest

    private(set) var handledIds: [String] = []
    private var waiters: [(threshold: Int, continuation: CheckedContinuation<Void, Never>)] = []

    nonisolated func canHandle(_: SSOTrUAPIRequest) -> Bool { true }

    func handle(message: SSOTrUAPIRequest, from _: PolkadotSignInHost) async {
        record(message.messageId)
    }

    func record(_ messageId: String) {
        handledIds.append(messageId)
        resumeSatisfiedWaiters()
    }

    func waitForHandled(count: Int) async {
        if handledIds.count >= count { return }
        await withCheckedContinuation { continuation in
            waiters.append((threshold: count, continuation: continuation))
        }
    }

    private func resumeSatisfiedWaiters() {
        waiters.removeAll { waiter in
            guard handledIds.count >= waiter.threshold else { return false }
            waiter.continuation.resume()
            return true
        }
    }
}

private final class StubAccountHolderService: NativeSsoAccountHolderServiceProtocol, @unchecked Sendable {
    var control: (Data) throws -> SsoRequestOutcome? = { _ in nil }
    var request: (Data) async throws -> SsoRequestOutcome = { _ in .ignored }
    var validate: () throws -> Void = {}

    func handleSsoControl(message: Data) throws -> SsoRequestOutcome? {
        try control(message)
    }

    func handleSsoRequest(message: Data) async throws -> SsoRequestOutcome {
        try await request(message)
    }

    func requireCurrentSession() throws {
        try validate()
    }
}

private actor RawMessageSender: PolkadotHostMessageSending {
    private(set) var postedMessages: [SSORawHostMessage] = []

    func setExchangeService(_: AnyMessageExchangeService<OpaqueSSORawHostMessage>) async {}

    func postMessage(_ message: SSORawHostMessage, to _: PolkadotSignInHost) async throws {
        postedMessages.append(message)
    }

    func handleDidPostMessages(_: [SSORawHostMessage], withError _: Error?) async {}

    func cancelPendingMessages(excluding _: Set<String>) async {}
}

private struct UnexpectedDisconnectApplier: SSORemoteDisconnectApplying {
    func applyDisconnect(from _: PolkadotSignInHost) async {
        Issue.record("The request must not disconnect its peer")
    }
}

// MARK: - Helpers

private func makeHost(accountId: Data = Data(repeating: 0x01, count: 32)) -> PolkadotSignInHost {
    PolkadotSignInHost(
        accountId: accountId,
        publicKey: Data(repeating: 0x02, count: 32),
        name: "TestHost",
        iconUrl: nil
    )
}

private func makeRawMessage(messageId: String, body: [UInt8] = [0xAA]) throws -> SSORawHostMessage {
    let encoder = ScaleEncoder()
    try messageId.encode(scaleEncoder: encoder)
    var data = encoder.encode()
    data.append(contentsOf: body)
    return try SSORawHostMessage(rawBytes: data)
}

private func makeMessageHandler(
    spy: SpyRequestHandler,
    repositoryFactory: InMemoryHandledRequestRepositoryFactory
) -> SSOTrUAPIMessageHandler {
    let context = SSORequestProcessingContext<SSOTrUAPIRequest>(
        handlers: [spy],
        logger: MockLogger()
    )
    return SSOTrUAPIMessageHandler(
        processingContext: context,
        handledRequestRepositoryFactory: repositoryFactory,
        logger: MockLogger()
    )
}

// MARK: - Tests

@Suite("SSOTrUAPIMessageHandler Tests")
struct SSOTrUAPIMessageHandlerTests {
    @Test("Fresh messages are enqueued into the context and marked handled")
    func freshMessagesEnqueuedAndMarked() async throws {
        let spy = SpyRequestHandler()
        let repositoryFactory = InMemoryHandledRequestRepositoryFactory()
        let handler = makeMessageHandler(spy: spy, repositoryFactory: repositoryFactory)

        let first = try makeRawMessage(messageId: "fresh-1")
        let second = try makeRawMessage(messageId: "fresh-2")

        await handler.handleMessages([first, second], from: makeHost(), service: StubAccountHolderService())
        await spy.waitForHandled(count: 2)

        let handledIds = await spy.handledIds
        #expect(handledIds == ["fresh-1", "fresh-2"])

        let marked = try await repositoryFactory.fetchAll().map(\.messageId)
        #expect(Set(marked) == ["fresh-1", "fresh-2"])
    }

    @Test("Already-handled messageId is filtered before reaching the context")
    func duplicateMessageIdFilteredBeforeContext() async throws {
        let spy = SpyRequestHandler()
        let repositoryFactory = InMemoryHandledRequestRepositoryFactory()
        try await repositoryFactory.seed(messageId: "dup")

        let handler = makeMessageHandler(spy: spy, repositoryFactory: repositoryFactory)

        let duplicate = try makeRawMessage(messageId: "dup")
        let fresh = try makeRawMessage(messageId: "fresh")

        await handler.handleMessages([duplicate, fresh], from: makeHost(), service: StubAccountHolderService())
        await spy.waitForHandled(count: 1)

        let handledIds = await spy.handledIds
        #expect(handledIds == ["fresh"])
    }

    @Test(
        "Cancel reaches an active request while ordinary requests keep FIFO order across peers",
        .timeLimit(.minutes(1))
    )
    func cancellationBypassesTheSharedQueue() async throws {
        let events = SpyRequestHandler()
        let sender = RawMessageSender()
        let firstService = StubAccountHolderService()
        let secondService = StubAccountHolderService()
        let cancellation = AsyncStream.makeStream(of: Void.self)
        var cancelWasHandled = false
        firstService.control = { message in
            guard try SSORawHostMessage(rawBytes: message).messageId == "cancel" else { return nil }
            cancelWasHandled = true
            cancellation.continuation.yield(())
            cancellation.continuation.finish()
            return .ignored
        }
        firstService.request = { message in
            let id = try SSORawHostMessage(rawBytes: message).messageId
            if id == "first" {
                await events.record("first-started")
                var iterator = cancellation.stream.makeAsyncIterator()
                _ = await iterator.next()
                await events.record("first-cancelled")
            } else {
                await events.record("first-peer:\(id)")
            }
            return .ignored
        }
        secondService.request = { message in
            let id = try SSORawHostMessage(rawBytes: message).messageId
            await events.record("second-peer:\(id)")
            return .ignored
        }
        let requestHandler = SSOTrUAPIRequestHandler(
            sender: sender,
            disconnectApplier: UnexpectedDisconnectApplier(),
            logger: MockLogger()
        )
        let repository = InMemoryHandledRequestRepositoryFactory()
        let handler = SSOTrUAPIMessageHandler(
            processingContext: SSORequestProcessingContext(handlers: [requestHandler], logger: MockLogger()),
            handledRequestRepositoryFactory: repository,
            logger: MockLogger()
        )
        try await handler.handleMessages(
            [makeRawMessage(messageId: "first")],
            from: makeHost(),
            service: firstService
        )
        await events.waitForHandled(count: 1)
        try await handler.handleMessages(
            [makeRawMessage(messageId: "second")],
            from: makeHost(accountId: Data(repeating: 3, count: 32)),
            service: secondService
        )
        try await handler.handleMessages(
            [makeRawMessage(messageId: "third"), makeRawMessage(messageId: "cancel")],
            from: makeHost(),
            service: firstService
        )
        #expect(cancelWasHandled)
        if !cancelWasHandled { cancellation.continuation.finish() }
        await events.waitForHandled(count: 4)
        #expect(await events.handledIds == [
            "first-started",
            "first-cancelled",
            "second-peer:second",
            "first-peer:third"
        ])
        let marked = try await repository.fetchAll().map(\.messageId)
        #expect(Set(marked) == ["first", "second", "third", "cancel"])
    }

    @Test("Responses can only start posting while their bound session is current", arguments: [false, true])
    func responsePostRequiresCurrentSession(stale: Bool) async throws {
        enum SessionError: Error { case stale }

        let service = StubAccountHolderService()
        let message = try makeRawMessage(messageId: "response")
        service.request = { _ in .response(message: message.rawBytes) }
        service.validate = {
            if stale { throw SessionError.stale }
        }
        let sender = RawMessageSender()
        let handler = SSOTrUAPIRequestHandler(
            sender: sender,
            disconnectApplier: UnexpectedDisconnectApplier(),
            logger: MockLogger()
        )

        await handler.handle(message: SSOTrUAPIRequest(message: message, service: service), from: makeHost())

        #expect(await sender.postedMessages == (stale ? [] : [message]))
    }
}
