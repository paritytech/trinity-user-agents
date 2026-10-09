import Foundation
import Testing
import SubstrateSdk
import Products

@testable import polkadot_app

@Suite("SSO request withdrawal Tests")
struct SSORequestWithdrawalTests {
    private let host = PolkadotSignInHost(
        accountId: Data(repeating: 0x01, count: 32),
        publicKey: Data(repeating: 0x02, count: 32),
        name: "TestHost",
        iconUrl: nil
    )

    private func makeRequest(_ messageId: String) -> PolkadotHostRemoteMessage {
        PolkadotHostRemoteMessage(
            messageId: messageId,
            versionedContent: .v1(.productSubtreeRequest(.init(productId: "browse.dot")))
        )
    }

    private func makeCancel(withdrawing withdrawnId: String) -> PolkadotHostRemoteMessage {
        PolkadotHostRemoteMessage(
            messageId: "cancel-\(withdrawnId)",
            versionedContent: .v1(.cancel(withdrawnMessageId: withdrawnId))
        )
    }

    private func makeResponse(to requestId: String) -> PolkadotHostRemoteMessage {
        PolkadotHostRemoteMessage(
            messageId: "response-\(requestId)",
            versionedContent: .v1(.productSubtreeResponse(requestMessageId: requestId, result: .failure("x")))
        )
    }

    private func makeMessageHandler(
        handler: GatedRequestHandler<PolkadotHostRemoteMessage>
    ) -> PolkadotHostMessageHandler {
        PolkadotHostMessageHandler(
            processingContext: SSORequestProcessingContext(handlers: [handler], logger: MockLogger()),
            handledRequestRepositoryFactory: InMemoryHandledRequestRepositoryFactory(),
            logger: MockLogger()
        )
    }

    @Test("Decodes a Cancel at variant index 24")
    func decodesCancel() throws {
        // RemoteMessage { message_id: "c1", V1(Cancel(Withdrawal { message_id: "m9" })) }
        let bytes = Data([0x08, 0x63, 0x31, 0x00, 0x18, 0x08, 0x6D, 0x39])
        let message = try PolkadotHostRemoteMessage(scaleDecoder: ScaleDecoder(data: bytes))

        #expect(message.messageId == "c1")
        guard case let .cancel(withdrawnMessageId) = message.latestContent() else {
            Issue.record("expected a Cancel")
            return
        }
        #expect(withdrawnMessageId == "m9")
    }

    @Test("A queued request that is withdrawn never runs")
    func queuedRequestIsDropped() async {
        let gated = GatedRequestHandler<PolkadotHostRemoteMessage>(gatedIds: ["busy"])
        let handler = makeMessageHandler(handler: gated)

        await handler.handleMessages([makeRequest("busy"), makeRequest("queued")], from: host)
        await gated.waitUntilStarted("busy")

        await handler.handleMessages([makeCancel(withdrawing: "queued"), makeRequest("next")], from: host)
        await gated.release("busy")
        await gated.waitUntilFinished("next")

        let started = await gated.startedIds
        #expect(started == ["busy", "next"])
    }

    @Test("A running request that is withdrawn is cancelled")
    func runningRequestIsCancelled() async {
        let gated = GatedRequestHandler<PolkadotHostRemoteMessage>(gatedIds: ["running"])
        let handler = makeMessageHandler(handler: gated)

        await handler.handleMessages([makeRequest("running")], from: host)
        await gated.waitUntilStarted("running")

        await handler.handleMessages([makeCancel(withdrawing: "running")], from: host)
        await gated.release("running")
        await gated.waitUntilFinished("running")

        let cancelled = await gated.cancelledIds
        #expect(cancelled == ["running"])
    }

    @Test("Withdrawing a running request closes its prompts")
    func runningRequestPromptsAreWithdrawn() async throws {
        let gated = GatedRequestHandler<PolkadotHostRemoteMessage>(gatedIds: ["running"])
        let handler = makeMessageHandler(handler: gated)

        await handler.handleMessages([makeRequest("running")], from: host)
        await gated.waitUntilStarted("running")
        let scope = try #require(await gated.promptScopes["running"])

        await handler.handleMessages([makeCancel(withdrawing: "running")], from: host)
        var isWithdrawn = await scope.isWithdrawn
        for _ in 0 ..< 100 where !isWithdrawn {
            await Task.yield()
            isWithdrawn = await scope.isWithdrawn
        }

        #expect(isWithdrawn)
        await gated.release("running")
    }

    @Test("A request withdrawn before it arrives never runs")
    func unseenRequestIsDropped() async {
        let gated = GatedRequestHandler<PolkadotHostRemoteMessage>()
        let handler = makeMessageHandler(handler: gated)

        await handler.handleMessages([makeCancel(withdrawing: "late")], from: host)
        await handler.handleMessages([makeRequest("late"), makeRequest("next")], from: host)
        await gated.waitUntilFinished("next")

        let started = await gated.startedIds
        #expect(started == ["next"])
    }

    @Test("Withdrawals are remembered up to capacity, oldest forgotten first")
    func withdrawalMemoryIsBounded() {
        let withdrawn = SSOWithdrawnRequests(capacity: 2)

        withdrawn.insert("a")
        withdrawn.insert("b")
        withdrawn.insert("c")

        #expect(!withdrawn.contains("a"))
        #expect(withdrawn.contains("b"))
        #expect(withdrawn.contains("c"))
    }

    @Test("A response to a withdrawn request is not posted")
    func responseToWithdrawnRequestIsDropped() async throws {
        let inner = MockHostMessageSender()
        let withdrawn = SSOWithdrawnRequests()
        let sender = SSOWithdrawalGuardedSender(
            sender: inner,
            withdrawnRequests: withdrawn,
            logger: MockLogger()
        )

        withdrawn.insert("withdrawn")
        try await sender.postMessage(makeResponse(to: "withdrawn"), to: host)
        try await sender.postMessage(makeResponse(to: "live"), to: host)

        #expect(inner.postedMessages.map(\.messageId) == ["response-live"])
    }
}
