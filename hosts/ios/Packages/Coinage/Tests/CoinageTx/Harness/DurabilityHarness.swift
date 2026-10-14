import DurableTransactions
import DurableTransactionsTestSupport
@preconcurrency import ExtrinsicService
import Foundation
import os
import SubstrateSdk
@testable import Coinage

/// Blocks a coinage extrinsic stays valid for by default in the harness.
let harnessMortalPeriod: UInt32 = 128

/// The chain the harness's coinage domain lives on.
let harnessChainId: ChainId = "harness-chain"

/// The whole durability subsystem over a ``CoinageFakeChain`` and an in-memory ledger.
///
/// It wires the *real* engine pieces — registrar, async submission tracker and recovery pass — over the
/// real coinage oracle and fakes, so the async submission path and the coinage rules are exercised for
/// real. The one deviation from production is deliberate: the tracker's release-time `onRecovery` only
/// *records* a request rather than launching a pass, so passes run only when a scenario asks — which
/// keeps every walk replayable and shrinkable.
///
/// ``crash()`` is the point of the harness: it drops every volatile set (the ownership set, the
/// registrar, the tracker, the pass) and builds the subsystem again over the same stores, which is only
/// possible because no volatile state is global. ``relaunch()`` additionally releases uncommitted
/// handoff marks, the way a process start does.
final class DurabilityHarness: @unchecked Sendable {
    let chain: CoinageFakeChain
    let chainFactory: FakePinnedChainViewFactory<CoinageChainState>
    let stateReader: FakeCoinageStateReader
    let store: MockCoinageTxRepository
    let submitter: FakeExtrinsicSubmitter

    /// Registered once and kept across ``crash()``, the way an app re-registers its policies on launch.
    /// Empty by default: a transaction without a policy fails for good, which is what most scenarios mean.
    let policies = DurableSubmissionPolicyRegistry()

    let backgroundExecutor = StubBackgroundExecutor()

    private var subsystem: Subsystem
    private var nextExtrinsicSeq: UInt64 = 0

    /// The bytes behind each registered `txHash`, so a submission can present the extrinsic whose
    /// hash the row actually carries.
    private var extrinsicBytesByHash: [Data: Data] = [:]
    private let pendingSubmissions = OSAllocatedUnfairLock(initialState: [CoinageTxId]())

    init(
        initialState: CoinageChainState = .empty,
        submitter: FakeExtrinsicSubmitter = FakeExtrinsicSubmitter()
    ) {
        chain = CoinageFakeChain(initialState: initialState)
        chainFactory = FakePinnedChainViewFactory(chain: chain)
        stateReader = FakeCoinageStateReader(chain: chain)
        store = MockCoinageTxRepository()
        self.submitter = submitter
        subsystem = Subsystem.build(
            store: store,
            chainFactory: chainFactory,
            stateReader: stateReader,
            submitter: submitter,
            backgroundExecutor: backgroundExecutor,
            policies: policies
        )
    }

    /// Where a retriable failure is turned into another attempt — the same writer the pass and the
    /// submission watch both go through.
    var verdictWriter: DurableVerdictWriter { subsystem.verdictWriter }

    /// Recovery being asked for is the observable half of a submission release.
    var recoveryRequestCount: Int { subsystem.recorder.count }

    /// Whether a live submission still owns the entry — the ledger lock a pass steps around.
    func isOwnedBySubmission(_ id: CoinageTxId) -> Bool { subsystem.owned.isOwned(id) }

    /// Reserves `assets` against being spent again, returning the commit handle. The two-phase form a
    /// scenario drives directly (``handOff(_:)`` is the pre-commit-and-commit shorthand). The same check
    /// the coinage service runs: a live claimant makes the pre-commit throw.
    func preCommitHandoff(_ assets: [OwnAsset]) async throws -> any CoinageHandoffCommit {
        let keys = Set(assets.map(\.publicKey))
        try await store.ledger.precommitHandOff(assets) { context in
            let claimed = try context.filterClaimed(keys)
            if let key = claimed.first {
                throw CoinageTxError.handoffOfClaimedAsset(key.toHex())
            }
        }
        return StoreHandoffCommit(assets: assets, ledger: store.ledger)
    }

    func crash() {
        subsystem = Subsystem.build(
            store: store,
            chainFactory: chainFactory,
            stateReader: stateReader,
            submitter: submitter,
            backgroundExecutor: backgroundExecutor,
            policies: policies
        )
        pendingSubmissions.withLock { $0.removeAll() }
    }

    /// What a relaunch does before anything else, so a scenario can model process start.
    func relaunch() async throws {
        crash()
        try await store.releaseUncommittedHandoffs()
    }

    // MARK: - Running the subsystem

    /// One recovery pass. Deterministic: nothing else launches a pass, so this is the only writer.
    func runPass() async {
        await subsystem.pass.run()
    }

    /// A pass with one read failing throughout it, so every evidence path has an unknown to handle.
    func runPass(withFault fault: FuzzFault) async {
        let healthyChain = chainFactory.faults
        let healthyCoinage = stateReader.faults
        await applyFault(fault)
        // Not the fault-free `runPass`: a pass that cannot pin returns without a verdict, which is the
        // outcome this is here to produce.
        await subsystem.pass.run()
        chainFactory.faults = healthyChain
        stateReader.faults = healthyCoinage
    }

    /// Runs the watchers to their release. A pass skips the entries submission still owns, so a
    /// scenario that wants the pass to decide has to get past this first. Awaits the real async
    /// tracker rather than advancing a virtual clock.
    func releaseSubmissions() async {
        let ids = pendingSubmissions.withLock { current -> [CoinageTxId] in
            let snapshot = current
            current.removeAll()
            return snapshot
        }
        guard !ids.isEmpty else { return }

        let owned = subsystem.owned
        var spins = 0
        while ids.contains(where: { owned.isOwned($0) }) {
            // Inside the loop, not once before it: a tracker's `Task` may not have reached
            // `submitAndSubscribe` (and parked its watch) yet when this is first called, so releasing
            // is retried every spin until the park exists and the emit lands — otherwise the tracker
            // waits out its silence timeout.
            submitter.releaseAll()
            await Task.yield()
            spins += 1
            if spins > 10_000 {
                try? await Task.sleep(nanoseconds: 100_000)
                spins = 0
            }
        }
        // Let the release-time `onRecovery` record run before returning, so a scenario can assert on it.
        await Task.yield()
        await Task.yield()
    }

    // MARK: - Extrinsic hashes

    /// A distinct extrinsic hash, and the bytes it is the hash *of*.
    ///
    /// The hash must really be `blake2b32` of the submitted bytes: a verdict is only written while the
    /// row's attempt still matches the bytes being watched, so a harness that registered an unrelated
    /// hash would have every write silently refused. Only distinctness matters otherwise — the body
    /// search looks an entry's hash up in block bodies, so two entries sharing bytes would find each
    /// other's blocks.
    func nextExtrinsicHash() -> Data {
        defer { nextExtrinsicSeq += 1 }
        var bytes = [UInt8](repeating: 0xEE, count: 32)
        var seq = nextExtrinsicSeq
        for offset in 0 ..< 8 {
            bytes[offset] = UInt8(truncatingIfNeeded: seq)
            seq >>= 8
        }

        let extrinsic = Data(bytes)
        // A harness fixture: a failure here means the hashing primitive changed, not a test condition.
        guard let hash = try? extrinsic.blake2b32() else {
            fatalError("harness could not hash its own extrinsic bytes")
        }

        extrinsicBytesByHash[hash] = extrinsic

        return hash
    }

    // MARK: - Registration

    /// Registers a batch atomically — the engine's rows and coinage's asset rows in one transaction —
    /// tracks each through the real async tracker, and returns the ids. Callers usually follow with
    /// ``releaseSubmissions()`` before running a pass.
    @discardableResult
    func submit(_ registrations: [CoinageTxRegistration]) async throws -> [CoinageTxId] {
        let ledger = store.ledger
        let assets = registrations.map(\.assets)
        let ids = try await subsystem.registrar.register(registrations.map(\.durable)) { scope, ids in
            try ledger.registerAssets(assets, for: ids, in: scope)
        }
        pendingSubmissions.withLock { $0.append(contentsOf: ids) }

        let recorder = subsystem.recorder
        let baseline = submitter.submissionCount
        for (id, registration) in zip(ids, registrations) {
            let submission = DurableTxTracker.Submission(
                model: harnessBuiltModel(hex: extrinsic(for: registration.txHash)),
                transactionId: id,
                chainId: harnessChainId,
                submitter: submitter
            )
            subsystem.tracker.track(submission) { recorder.record() }
        }

        // Wait until the trackers have parked their watches, so submission index order matches
        // registration order — a watcher scenario keys its status streams by submission index.
        await awaitParked(untilCount: baseline + ids.count)
        return ids
    }

    /// The bytes whose hash is `txHash`, as hex — what a submission presents for that row.
    private func extrinsic(for txHash: Data) -> String {
        guard let bytes = extrinsicBytesByHash[txHash] else {
            fatalError("no extrinsic recorded for \(txHash.toHex()) — build it through nextExtrinsicHash()")
        }

        return bytes.toHex(includePrefix: true)
    }

    private func awaitParked(untilCount target: Int) async {
        var spins = 0
        while submitter.submissionCount < target {
            await Task.yield()
            spins += 1
            if spins > 10_000 {
                try? await Task.sleep(nanoseconds: 100_000)
                spins = 0
            }
        }
    }

    /// A registration anchored at the current finalized head, which is what the registrar reads its
    /// window from.
    func registration(
        inputs: [CoinageTxInput],
        outputs: [OwnAsset],
        period: UInt32,
        groupId: CoinageTxGroupId? = nil
    ) -> CoinageTxRegistration {
        let finalized = chain.finalizedHead
        return CoinageTxRegistration(
            txHash: nextExtrinsicHash(),
            checkpoint: BlockRef(number: finalized.number, hash: finalized.hash),
            mortalityBlocks: period,
            groupId: groupId,
            inputs: inputs,
            outputs: outputs
        )
    }

    // MARK: - Handoff

    /// Pre-commits and commits a handoff of `assets`, returning whether it committed (a live claimant
    /// makes the pre-commit throw).
    @discardableResult
    func handOff(_ assets: [OwnAsset]) async -> Bool {
        do {
            let commit = try await preCommitHandoff(assets)
            try commit.commit(in: InMemoryRegistrationScope())
            return true
        } catch {
            return false
        }
    }

    // MARK: - Voucher / alias helpers

    /// The alias storage key the collector would ask for this voucher, derived from the chain exactly
    /// as it derives it — `nil` when the voucher is in no ring, so it has no alias key.
    func currentAliasKey(index: CoinageKeyIndex) -> FakeAliasKey? {
        let member = HarnessKeys.voucherMemberKey(index)
        let state = chain.bestHead.state
        guard let exponent = state.recyclerMembers[member],
              let ringIndex = state.ringPositions[member]?.ringIndex
        else { return nil }
        return CoinageChainState.aliasKey(index: index, exponent: exponent, ringIndex: ringIndex)
    }

    // MARK: - Faults

    private func applyFault(_ fault: FuzzFault) async {
        switch fault {
        case .coins:
            stateReader.faults.statelessBlocks.formUnion([chain.finalizedHead.hash, chain.bestHead.hash])
        case .aliases:
            await stateReader.faults.unreadableAliases.formUnion(allCurrentAliasKeys())
        case .memberships:
            stateReader.faults.membershipsUnreadable = true
        case .ringPositions:
            stateReader.faults.ringPositionsUnreadable = true
        case .blocks:
            chainFactory.faults.everyBlockUnreadable = true
        case .outcomes:
            let entries = await (try? store.getAllEntries()) ?? []
            // Only a row with an attempt has an outcome to make unreadable.
            chainFactory.faults.unreadableOutcomes.formUnion(entries.compactMap(\.txHash))
        case .pin:
            chainFactory.faults.pinFails = true
        }
    }

    /// The current alias keys of every voucher any entry references, so an alias fault silences the
    /// reads a pass would actually make.
    private func allCurrentAliasKeys() async -> Set<FakeAliasKey> {
        let entries = await (try? store.getAllEntries()) ?? []
        let indices = entries.flatMap { entry -> [CoinageKeyIndex] in
            let fromInputs = entry.inputs.compactMap { input -> CoinageKeyIndex? in
                if case let .recyclerVoucher(index, _) = input { return index }
                return nil
            }
            let fromOutputs = entry.outputs.compactMap { output -> CoinageKeyIndex? in
                if case let .recyclerVoucher(index, _) = output { return index }
                return nil
            }
            return fromInputs + fromOutputs
        }
        return Set(indices.compactMap { currentAliasKey(index: $0) })
    }
}

// MARK: - Subsystem

private extension DurabilityHarness {
    /// Records release-time recovery requests. Per-subsystem, so a crashed subsystem's stale tracker
    /// tasks cannot pollute the live counter.
    final class RecoveryRecorder: @unchecked Sendable {
        private let lock = OSAllocatedUnfairLock(initialState: 0)
        func record() { lock.withLock { $0 += 1 } }
        var count: Int { lock.withLock { $0 } }
    }

    struct Subsystem {
        let owned: DurableTxOwnershipSet
        let registrar: DurableTxRegistrar
        let tracker: DurableTxTracker
        let pass: DurableRecoveryPass
        let verdictWriter: DurableVerdictWriter
        let recorder: RecoveryRecorder

        static func build(
            store: MockCoinageTxRepository,
            chainFactory: FakePinnedChainViewFactory<CoinageChainState>,
            stateReader: FakeCoinageStateReader,
            submitter _: FakeExtrinsicSubmitter,
            backgroundExecutor: StubBackgroundExecutor,
            policies: DurableSubmissionPolicyRegistry
        ) -> Subsystem {
            let owned = DurableTxOwnershipSet()
            let recorder = RecoveryRecorder()
            let oracles = TxCompletionOracleRegistry()
            oracles.register(
                CoinageResourceOracle(chainId: harnessChainId, ledger: store.ledger, reader: stateReader),
                for: .coinage
            )
            let registrar = DurableTxRegistrar(store: store.durable, owned: owned, logger: nil)
            let verdictWriter = DurableVerdictWriter(
                store: store.durable,
                policies: policies,
                logger: nil
            )
            let pass = DurableRecoveryPass(
                store: store.durable,
                chainFactory: chainFactory,
                owned: owned,
                oracles: oracles,
                verdictWriter: verdictWriter,
                logger: nil
            )
            let tracker = DurableTxTracker(
                store: store.durable,
                chainFactory: chainFactory,
                owned: owned,
                verdictWriter: verdictWriter,
                backgroundExecutor: backgroundExecutor,
                logger: nil
            )
            return Subsystem(
                owned: owned,
                registrar: registrar,
                tracker: tracker,
                pass: pass,
                verdictWriter: verdictWriter,
                recorder: recorder
            )
        }
    }
}
