import AsyncExtensions
import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import KeyDerivation
import StructuredConcurrency
import Testing
@testable import Coinage

@Suite("Native Coinage root and activation fence")
struct CoinageLifecycleTests {
    @Test("Persisted incoming ownership survives recreation but separates roots, chains and instances")
    func persistentOwnerIsolation() throws {
        let root = LifecycleTestRoot()
        let original = try CoinageLifecycle(rootEntropyManager: root)
        let owner = original.ownerId(chainId: "chain", instanceId: 1)
        let recreated = try CoinageLifecycle(rootEntropyManager: root)
        #expect(recreated.ownerId(chainId: "chain", instanceId: 1) == owner)
        #expect(original.setActive(true))
        original.setActive(false)
        #expect(original.setActive(true))
        #expect(original.ownerId(chainId: "chain", instanceId: 1) == owner)
        #expect(original.ownerId(chainId: "other-chain", instanceId: 1) != owner)
        #expect(original.ownerId(chainId: "chain", instanceId: 2) != owner)

        try root.createRootEntropy(Data(repeating: 2, count: 32))
        let replacement = try CoinageLifecycle(rootEntropyManager: root)
        #expect(replacement.ownerId(chainId: "chain", instanceId: 1) != owner)
        // A retired service never relabels its pending source custody as the replacement wallet's.
        #expect(original.ownerId(chainId: "chain", instanceId: 1) == owner)
    }

    @Test("Coin and voucher derivations reject a replacement root; Keychain lock is not bypassed")
    func rootBoundDerivation() throws {
        let root = LifecycleTestRoot()
        let lifecycle = try CoinageLifecycle(rootEntropyManager: root)
        #expect(lifecycle.setActive(true))
        let coins = CoinKeypairFactory(entropyManager: lifecycle)
        let vouchers = VoucherKeypairFactory(entropyManager: lifecycle)
        let originalCoin = try coins.derivePublicKey(index: 1)
        let originalVoucher = try vouchers.derivePublicKey(index: 1)

        root.setLocked(true)
        #expect(throws: LifecycleTestRoot.Failure.locked) { try coins.derivePublicKey(index: 1) }
        #expect(throws: LifecycleTestRoot.Failure.locked) { try vouchers.derivePublicKey(index: 1) }
        root.setLocked(false)
        #expect(try coins.derivePublicKey(index: 1) == originalCoin)
        #expect(try vouchers.derivePublicKey(index: 1) == originalVoucher)

        try root.createRootEntropy(Data(repeating: 2, count: 32))
        #expect(throws: CoinageLifecycleError.rootChanged) { try coins.derivePublicKey(index: 1) }
        #expect(throws: CoinageLifecycleError.rootChanged) { try vouchers.derivePublicKey(index: 1) }
        #expect(!lifecycle.setActive(true))
        let replacement = try CoinageLifecycle(rootEntropyManager: root)
        #expect(replacement.setActive(true))
        #expect(try CoinKeypairFactory(entropyManager: replacement).derivePublicKey(index: 1) != originalCoin)
    }

    @Test("An unstructured queued operation cannot derive after deactivation and same-root reactivation")
    func suspendedDerivationCannotRevive() async throws {
        let lifecycle = try CoinageLifecycle(rootEntropyManager: LifecycleTestRoot())
        #expect(lifecycle.setActive(true))
        let coins = CoinKeypairFactory(entropyManager: lifecycle)
        let original = try coins.derivePublicKey(index: 1)
        let queue = SerialOperationQueue()
        let gate = LifecycleRegistrationGate()
        let pending = Task {
            try await lifecycle.withOperation {
                try await queue.run {
                    await gate.pause()
                    return try coins.derivePublicKey(index: 1)
                }
            }
        }
        await gate.waitUntilPaused()
        lifecycle.setActive(false)
        pending.cancel()
        #expect(lifecycle.setActive(true))
        await gate.resume()
        await #expect(throws: CoinageLifecycleError.staleOperation) { try await pending.value }
        #expect(try coins.derivePublicKey(index: 1) == original)
    }

    @Test("Replacement after transaction preparation rejects the native durable registration hook")
    func replacementBeforeRegistration() async throws {
        let root = LifecycleTestRoot()
        let lifecycle = try CoinageLifecycle(rootEntropyManager: root)
        #expect(lifecycle.setActive(true))
        let engine = LifecycleRegistrationEngine()
        let service = CoinageTxService(engine: engine, ledger: engine.store.ledger, logger: nil, lifecycle: lifecycle)
        let request = request()
        let pending = Task { try await service.submitTransactions([request], groupId: "old-root") }
        await engine.gate.waitUntilPaused()
        try root.createRootEntropy(Data(repeating: 2, count: 32))
        await engine.gate.resume()

        await #expect(throws: CoinageLifecycleError.rootChanged) { try await pending.value }
        #expect(engine.store.durable.allEntries.isEmpty)
        #expect(try await engine.store.ledger.getAllEntries().isEmpty)
        #expect(engine.store.handoffMarks.isEmpty)
    }

    @Test("Reactivation cannot register a suspended operation; a fresh operation can register")
    func reactivationBeforeRegistration() async throws {
        let lifecycle = try CoinageLifecycle(rootEntropyManager: LifecycleTestRoot())
        #expect(lifecycle.setActive(true))
        let engine = LifecycleRegistrationEngine()
        let service = CoinageTxService(engine: engine, ledger: engine.store.ledger, logger: nil, lifecycle: lifecycle)
        let request = request()
        let pending = Task { try await service.submitTransactions([request], groupId: "suspended") }
        await engine.gate.waitUntilPaused()
        lifecycle.setActive(false)
        #expect(lifecycle.setActive(true))
        await engine.gate.resume()

        await #expect(throws: CoinageLifecycleError.staleOperation) { try await pending.value }
        #expect(engine.store.durable.allEntries.isEmpty)
        #expect(try await engine.store.ledger.getAllEntries().isEmpty)
        let ids = try await service.submitTransactions([request], groupId: "fresh")
        let id = try #require(ids.first)
        #expect(try await service.getOperationGroupStatuses("suspended").isEmpty)
        let registered = try #require(try await engine.store.ledger.getEntry(id: id))
        #expect(registered.outputs == request.outputs)
        #expect(registered.groupId == "fresh")
    }

    private func request() -> CoinageTxRequest {
        CoinageTxRequest(
            inputs: [.coin(.own(1, testKey(1)))], outputs: [.coin(2, testKey(2))],
            builder: { $0 }, origin: StubExtrinsicOrigin()
        )
    }
}

private final class LifecycleTestRoot: RootEntropyManaging, @unchecked Sendable {
    enum Failure: Error, Equatable { case locked }
    private let lock = NSLock()
    private var entropy = Data(repeating: 1, count: 32)
    private var locked = false

    func setLocked(_ value: Bool) { lock.withLock { locked = value } }
    func fetchRootEntropy() throws -> Data {
        try lock.withLock {
            if locked { throw Failure.locked }
            return entropy
        }
    }
    func createRootEntropy(_ value: Data) throws { lock.withLock { entropy = value } }
    func hasRootEntropy() throws -> Bool { true }
}

private actor LifecycleRegistrationGate {
    private var paused = false
    private var open = false
    private var entered: CheckedContinuation<Void, Never>?
    private var release: CheckedContinuation<Void, Never>?

    func pause() async {
        guard !open else { return }
        await withCheckedContinuation {
            release = $0
            paused = true
            entered?.resume()
            entered = nil
        }
    }
    func waitUntilPaused() async {
        guard !paused else { return }
        await withCheckedContinuation { entered = $0 }
    }
    func resume() {
        open = true
        release?.resume()
        release = nil
    }
}

/// Supplies prepared extrinsics after a controllable suspension, then invokes the real registrar and
/// native CoinageTxService hook. The in-memory repository rolls back a throwing hook like CoreData.
private final class LifecycleRegistrationEngine: DurableTxServicing, @unchecked Sendable {
    let oracles = TxCompletionOracleRegistry()
    let store = MockCoinageTxRepository()
    let gate = LifecycleRegistrationGate()

    func submitTransactions(
        domain: TxDomainId,
        requests: [DurableTxRequest],
        groupId: DurableTxGroupId?,
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        await gate.pause()
        let registrations = requests.map { _ in CoinageTxRegistration.fixture(groupId: groupId).durable }
        return try await DurableTxRegistrar(store: store.durable, owned: DurableTxOwnershipSet())
            .register(registrations, onRegister: onRegister)
    }
    func subscribeTransactionStatus(_ id: DurableTxId) -> AnyAsyncSequence<DurableTxStatus> {
        store.durable.subscribeStatus(id: id)
    }
    func getGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) async throws -> [DurableTxEntry] {
        try await store.durable.getGroupEntries(domain: domain, groupId: groupId)
    }
    func subscribeGroupEntries(domain: TxDomainId, groupId: DurableTxGroupId) -> AnyAsyncSequence<[DurableTxEntry]> {
        store.durable.subscribeGroupEntries(domain: domain, groupId: groupId)
    }
    func startRecoveryPass() {}
    func start() {}
    func stop() {}
}
