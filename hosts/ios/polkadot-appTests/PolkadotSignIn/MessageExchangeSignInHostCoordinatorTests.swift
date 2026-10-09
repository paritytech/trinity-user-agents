import Foundation
import Testing
import MessageExchangeKit
import Products

@testable import polkadot_app

/// Holds each resource allocation it sees until released, so a test can
/// withdraw a request while its handler is mid-allocation.
private actor AllocationGate {
    private var enteredCount = 0
    private var entryWaiters: [(count: Int, continuation: CheckedContinuation<Void, Never>)] = []
    private var held: [CheckedContinuation<Void, Never>] = []
    private var isReleased = false

    func pass() async {
        enteredCount += 1
        entryWaiters.removeAll { waiter in
            guard enteredCount >= waiter.count else { return false }
            waiter.continuation.resume()
            return true
        }

        guard !isReleased else { return }
        await withCheckedContinuation { held.append($0) }
    }

    func waitUntilEntered(_ count: Int) async {
        guard enteredCount < count else { return }
        await withCheckedContinuation { entryWaiters.append((count: count, continuation: $0)) }
    }

    /// Lets the allocations held so far finish; later ones are held again.
    func releaseHeld() {
        held.forEach { $0.resume() }
        held.removeAll()
    }

    func releaseAll() {
        isReleased = true
        releaseHeld()
    }
}

@Suite("MessageExchangeSignInHostCoordinator Tests")
struct MessageExchangeSignInHostCoordinatorTests {
    private let host = PolkadotSignInHost(
        accountId: Data(repeating: 0x01, count: 32),
        publicKey: Data(repeating: 0x02, count: 32),
        name: "TestHost",
        iconUrl: nil
    )

    private func makeAllocationRequest(_ messageId: String) -> PolkadotHostRemoteMessage {
        PolkadotHostRemoteMessage(
            messageId: messageId,
            versionedContent: .v1(.resourceAllocationRequest(.init(
                callingProduct: "test.product",
                resources: [.statementStoreAllowance],
                onExisting: .ignore
            )))
        )
    }

    private func makeCancel(withdrawing withdrawnId: String) -> PolkadotHostRemoteMessage {
        PolkadotHostRemoteMessage(
            messageId: "cancel-\(withdrawnId)",
            versionedContent: .v1(.cancel(withdrawnMessageId: withdrawnId))
        )
    }

    @Test("A withdrawn request's handler posts no response through the coordinator's sender")
    func withdrawnRequestPostsNoResponse() async {
        let gate = AllocationGate()
        let accountManager = MockProductsAccountManager()
        accountManager.onRequestResourceAllocation = { await gate.pass() }

        let chainRegistry = MockChainRegistry()
        chainRegistry.connectionsByChainId = [AppConfig.Chains.chatChain: MockChainConnection()]

        let (hostsRegistered, hostsRegisteredContinuation) = AsyncStream<Void>.makeStream()
        let serviceFactory = StubMessageExchangeServiceFactory()
        serviceFactory.onUpdateSessions = { _ in hostsRegisteredContinuation.yield() }

        let messageSender = MockHostMessageSender()
        let coordinator = MessageExchangeSignInHostCoordinator(
            ownKeyId: Chat.Contact.Own(signKeyId: "", encryptionKeyId: ""),
            serviceFactory: serviceFactory,
            accountManager: accountManager,
            sponsorFactory: StubTransactionSponsors(),
            routers: ProductRoutersFacade.sso(),
            chainRegistry: chainRegistry,
            hostsDataProviderFactory: StubPolkadotSignInHostDataProviderFactory(hosts: [host]),
            hostRepositoryFactory: MockHostRepositoryFactory(),
            messageSender: messageSender,
            handledRequestRepositoryFactory: InMemoryHandledRequestRepositoryFactory(),
            logger: MockLogger()
        )
        let peer = MessageExchange.Peer(accountId: host.accountId, publicKey: host.publicKey, pin: nil, devices: [])

        await coordinator.setup()
        var registrations = hostsRegistered.makeAsyncIterator()
        await registrations.next()

        await coordinator.handleIncomingMessages([makeAllocationRequest("withdrawn")], from: peer) { _ in }
        await gate.waitUntilEntered(1)

        await coordinator.handleIncomingMessages(
            [makeCancel(withdrawing: "withdrawn"), makeAllocationRequest("next")],
            from: peer
        ) { _ in }
        await gate.releaseHeld()
        await gate.waitUntilEntered(2)

        #expect(messageSender.postedMessages.compactMap(\.respondingTo).isEmpty)
        await gate.releaseAll()
    }
}
