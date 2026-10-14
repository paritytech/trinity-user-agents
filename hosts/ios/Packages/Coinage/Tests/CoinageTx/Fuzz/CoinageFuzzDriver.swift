import Foundation
@testable import Coinage
import DurableTransactions

/// Random walks over coins, vouchers and reorgs, checked against invariants rather than expected
/// verdicts.
///
/// A walk has no expected outcome, so the oracle is what the chain itself says: the ``FakeChain`` is
/// ground truth, and every claim the ledger makes is checked against it. `pending` is always allowed —
/// this catches a verdict that is confident and wrong, never one that is merely undecided.
///
/// Actions are drawn from ``enabledActions()``, which only ever offers transitions the runtime could
/// produce: an extrinsic is never applied outside its mortality era, and a failed dispatch leaves
/// untouched everything this subsystem reads (the runtime restores the coin in `post_dispatch`, and the
/// aliases an unload would have written roll back with the dispatch).
final class CoinageFuzzDriver {
    let harness: DurabilityHarness

    private var nextCoin: CoinageKeyIndex = firstOutputCoin
    /// Addresses are never reused, so a voucher index is offered for minting at most once.
    private var mintedVouchers: Set<CoinageKeyIndex> = []

    /// Clean passes each entry has seen since its window closed / since its transaction settled below
    /// the finalized head — counted rather than requiring two back-to-back, since passes are rare among
    /// many action kinds.
    private var expiredPasses: [Int64: Int] = [:]
    private var settledPasses: [Int64: Int] = [:]

    /// Marks whose keys really did leave, so a relaunch must keep them however it clears uncommitted ones.
    private var committedMarks: Set<PublicKey> = []
    private var terminalSeen: [Int64: TerminalVerdict] = [:]

    /// The incrementally maintained canonical-inclusion index. See ``canonicalTransactions()``.
    private var canonicalIndex: [Data: CanonicalInclusion] = [:]
    private var indexedUpTo: Int64 = -1
    private var indexedHash: Data?

    init(harness: DurabilityHarness) {
        self.harness = harness
    }

    func walk(_ rng: inout SplitMix64, steps: Int, profile: FuzzProfile) async throws -> [FuzzAction] {
        var taken: [FuzzAction] = []
        for _ in 0 ..< steps {
            let enabled = try await enabledActions()
            guard let action = profile.pick(&rng, from: enabled) else { continue }
            try await apply(action)
            taken.append(action)
            try await checkInvariants(taken)
        }
        return taken
    }

    /// Replays a recorded walk exactly, or gives up. An action that is not enabled means this candidate
    /// is no longer the walk that was recorded — coin indices and entry sequences are handed out in
    /// order, so dropping a registration renumbers everything after it — so the shrinker must not judge
    /// a different history than the one it is reducing.
    func replay(_ actions: [FuzzAction]) async throws {
        for (index, action) in actions.enumerated() {
            let enabled = try await enabledActions()
            guard enabled.contains(action) else { throw ReplayDiverged() }
            try await apply(action)
            try await checkInvariants(Array(actions.prefix(index + 1)))
        }
    }
}

// MARK: - Enabled actions

private extension CoinageFuzzDriver {
    func enabledActions() async throws -> [FuzzAction] {
        var actions: [FuzzAction] = [.produceBlock, .finalizeToBest, .runPass]
        for fault in FuzzFault.allCases {
            actions.append(.runPassWithFault(fault))
        }

        // A reorg can never cross the finalized head.
        for depth in harness.chain.reorgDepths {
            actions.append(.reorg(depth: depth))
        }

        let entries = try await harness.store.getAllEntries()
        let claimed = Set(entries.filter { $0.status != .failure }.flatMap { entry in entry.inputs.map(\.publicKey) })

        // Read once: what the chain holds does not change while the enabled set is built.
        let presentKeys = coinKeysOnBestChain()
        let present = coinsOnBestChain(presentKeys)
        let unclaimed = present.filter { !claimed.contains(HarnessKeys.coinKey($0)) }

        for coin in unclaimed {
            actions.append(.registerSpend(coin: coin))
            actions.append(.registerOffboard(coin: coin))
            actions.append(.registerSplit(coin: coin))
        }

        // A voucher can be unloaded only where the pallet would allow it: still a recycler member,
        // placed in a ring, and not already unloaded there.
        let unloadable = voucherPool
            .filter { unloadableOnBestChain($0) && !claimed.contains(HarnessKeys.voucherMemberKey($0)) }
        for voucher in unloadable {
            actions.append(.registerUnload(voucher: voucher))
        }
        if unloadable.count >= 2 {
            actions.append(.registerMultiUnload(first: unloadable[0], second: unloadable[1]))
        }

        for voucher in voucherPool where isMemberOnBestChain(voucher) {
            actions.append(.archiveRecycler(voucher: voucher))
        }
        for voucher in voucherPool where isOnboardingOnBestChain(voucher) {
            for ring in rings {
                actions.append(.placeVoucherInRing(voucher: voucher, ring: ring))
            }
        }

        let handedOff = try await harness.store.getHandoffKeys()
        let spendable = present.filter {
            !claimed.contains(HarnessKeys.coinKey($0)) && !handedOff.contains(HarnessKeys.coinKey($0))
        }
        for coin in spendable {
            actions.append(.handOff(coin: coin))
        }
        for coin in present where handedOff.contains(HarnessKeys.coinKey(coin)) {
            actions.append(.peerSpends(coin: coin))
        }
        if spendable.count >= 2 {
            actions.append(.registerBatch(first: spendable[0], second: spendable[1]))
        }

        actions.append(.crash)
        actions.append(.relaunch)

        // Loading a recycler is the only way a voucher comes into being; without it the pool only
        // shrinks. Only an index the chain has never held.
        if let free = mintableVouchers.first(where: { !mintedVouchers.contains($0) }) {
            for coin in present where !claimed.contains(HarnessKeys.coinKey(coin)) {
                actions.append(.registerVoucherMint(coin: coin, voucher: free))
            }
            actions.append(.registerExternalLoad(voucher: free))
        }

        // A block may carry an extrinsic only where the runtime would accept it: inside its era, not
        // already applied, and with its input still there to be taken as the origin.
        for entry in entries where entry.status.isLive
            && !isIncludedAnywhere(entry)
            && withinEra(entry)
            && inputsSpendableOnBestChain(entry, coins: presentKeys)
            && entry.outputs.allSatisfy({ !presentKeys.contains($0.publicKey) }) {
            actions.append(.includeTx(sequence: entry.sequence, success: true))
            actions.append(.includeTx(sequence: entry.sequence, success: false))
        }

        return actions
    }
}

// MARK: - Applying actions

private extension CoinageFuzzDriver {
    func apply(_ action: FuzzAction) async throws {
        switch action {
        case .produceBlock:
            harness.advanceBlocks(1, finality: .inBest)
        case .finalizeToBest:
            harness.finalizeToBest()
        case .runPass:
            await harness.runPass()
        case let .runPassWithFault(fault):
            await harness.runPass(withFault: fault)
        case let .reorg(depth):
            harness.reorgLastBlocks(depth)
        case let .registerSpend(coin):
            let output = takeCoin()
            try await register { try await self.harness.register(
                inputCoin: coin,
                outputCoin: output,
                period: fuzzMortalPeriod
            ) }
        case let .registerVoucherMint(coin, voucher):
            try await register { try await self.harness.registerVoucherMint(
                inputCoin: coin,
                voucher: voucher,
                period: fuzzMortalPeriod
            ) }
            // Consumes the voucher index whether or not the registration was accepted, so it is never
            // offered again — a fresh address is derived every time.
            mintedVouchers.insert(voucher)
        case let .registerExternalLoad(voucher):
            try await register {
                try await self.harness.registerExternalLoad(voucher: voucher, period: fuzzMortalPeriod)
            }
            mintedVouchers.insert(voucher)
        case let .registerUnload(voucher):
            let output = takeCoin()
            try await register { try await self.harness.registerVoucherUnload(
                vouchers: [voucher],
                outputCoin: output,
                period: fuzzMortalPeriod
            ) }
        case let .registerOffboard(coin):
            try await register { try await self.harness.registerOffboard(inputCoin: coin, period: fuzzMortalPeriod) }
        case let .registerSplit(coin):
            let first = takeCoin()
            let second = takeCoin()
            try await register { try await self.harness.registerSplit(
                inputCoin: coin,
                outputCoins: [first, second],
                period: fuzzMortalPeriod
            ) }
        case let .registerMultiUnload(first, second):
            let output = takeCoin()
            try await register { try await self.harness.registerVoucherUnload(
                vouchers: [first, second],
                outputCoin: output,
                period: fuzzMortalPeriod
            ) }
        case let .registerBatch(first, second):
            let firstOut = takeCoin()
            let secondOut = takeCoin()
            try await register {
                _ = try await self.harness.registerGroup(
                    pairs: [(input: first, output: firstOut), (input: second, output: secondOut)],
                    groupId: "fuzz-group-\(first)-\(second)",
                    period: fuzzMortalPeriod
                )
            }
        case let .handOff(coin):
            if await harness.handOff([harness.coinOutput(coin)]) {
                committedMarks.insert(HarnessKeys.coinKey(coin))
            }
        case let .peerSpends(coin):
            harness.chain.produceBlock { $0.consumeCoin(HarnessKeys.coinKey(coin)) }
        case .crash:
            harness.crash()
        case .relaunch:
            try await harness.relaunch()
        case let .placeVoucherInRing(voucher, ring):
            harness.placeVoucherInRing(voucher, ring: ring, finality: .inBest)
        case let .archiveRecycler(voucher):
            harness.archiveRecyclerOf(voucher, finality: .inBest)
        case let .includeTx(sequence, success):
            let entry = try await entryOf(sequence)
            harness.includeEntry(entry, success: success, finality: .inBest)
        }
    }

    /// Runs a registration and releases the tracker, dropping a store rejection the way a real caller
    /// sees a failed submit: a coin the chain still shows free but a handoff mark or another entry now
    /// covers is a legal race, not a harness break. Any non-rejection error propagates.
    func register(_ body: () async throws -> Void) async throws {
        do {
            try await body()
        } catch let error as CoinageTxError {
            _ = error
        }
        await harness.releaseSubmissions()
    }

    func takeCoin() -> CoinageKeyIndex {
        defer { nextCoin = nextCoin.next() }
        return nextCoin
    }

    func entryOf(_ sequence: Int64) async throws -> CoinageTxEntry {
        let entries = try await harness.store.getAllEntries()
        guard let entry = entries.first(where: { $0.sequence == sequence }) else {
            throw FuzzHarnessError.noEntry(sequence)
        }
        return entry
    }
}

// MARK: - Ground truth read from the chain

private extension CoinageFuzzDriver {
    var bestState: CoinageChainState { harness.chain.bestHead.state }

    func coinKeysOnBestChain() -> Set<PublicKey> { Set(bestState.coins.keys) }

    func coinKeysAtFinalizedHead() -> Set<PublicKey> { Set(harness.chain.finalizedHead.state.coins.keys) }

    /// The coin indices — seeds plus every output coin handed out so far — the chain currently holds.
    func coinsOnBestChain(_ keys: Set<PublicKey>) -> [CoinageKeyIndex] {
        (seedCoins + (firstOutputCoin.item ..< nextCoin.item).map(CoinageKeyIndex.harness))
            .filter { keys.contains(HarnessKeys.coinKey($0)) }
    }

    func isMemberOnBestChain(_ voucher: CoinageKeyIndex) -> Bool {
        bestState.recyclerMembers[HarnessKeys.voucherMemberKey(voucher)] != nil
    }

    func isOnboardingOnBestChain(_ voucher: CoinageKeyIndex) -> Bool {
        bestState.ringPositions[HarnessKeys.voucherMemberKey(voucher)] == .onboarding
    }

    /// Unloadable at the best head: a recycler member, in a ring, with no alias saying it was already
    /// unloaded.
    func unloadableOnBestChain(_ voucher: CoinageKeyIndex) -> Bool {
        guard isMemberOnBestChain(voucher), let aliasKey = harness.currentAliasKey(index: voucher) else {
            return false
        }
        return bestState.aliases[aliasKey] == nil
    }

    func inputsSpendableOnBestChain(_ entry: CoinageTxEntry, coins: Set<PublicKey>) -> Bool {
        entry.inputs.allSatisfy { input in
            if input.isCoin { return coins.contains(input.publicKey) }
            guard case let .recyclerVoucher(index, _) = input else { return false }
            return unloadableOnBestChain(index)
        }
    }

    func isIncludedAnywhere(_ entry: CoinageTxEntry) -> Bool {
        canonicalTransactions()[entry.submittedAttempt.txHash] != nil
    }

    /// The transaction can no longer execute, judged at the finalized head. Stated in the chain's terms
    /// rather than by reusing the rules' own predicate, so this stays an independent check.
    func windowClosed(_ entry: CoinageTxEntry) -> Bool {
        UInt64(harness.chain.finalizedHead.number) > UInt64(entry.submittedAttempt.checkpoint.number) +
            UInt64(entry.submittedAttempt.mortalityBlocks)
    }

    /// The block that would carry the extrinsic is inside its era. Judged on the block about to be
    /// produced (best + 1), not the current head, which the runtime would reject.
    func withinEra(_ entry: CoinageTxEntry) -> Bool {
        let next = UInt64(harness.chain.bestHead.number) + 1
        let anchor = UInt64(entry.submittedAttempt.checkpoint.number)
        let end = anchor + UInt64(entry.submittedAttempt.mortalityBlocks)
        return next >= anchor && next <= end
    }

    /// Every transaction the canonical chain carries, with where it landed and how it dispatched.
    /// Maintained as the walk runs rather than rebuilt each look — rebuilding walked every block for
    /// every action, quadratic in a walk's length. A reorg replaces already-folded blocks, so on
    /// detecting one (by hash, not height) the index is rebuilt rather than repaired.
    func canonicalTransactions() -> [Data: CanonicalInclusion] {
        let chain = harness.chain

        if indexedUpTo >= 0, chain.canonicalAt(UInt32(indexedUpTo))?.hash != indexedHash {
            canonicalIndex.removeAll()
            indexedUpTo = -1
            indexedHash = nil
        }

        while indexedUpTo < Int64(chain.bestHead.number) {
            guard let block = chain.canonicalAt(UInt32(indexedUpTo + 1)) else { break }
            for txHash in block.body where canonicalIndex[txHash] == nil {
                canonicalIndex[txHash] = CanonicalInclusion(
                    blockNumber: block.number,
                    outcome: block.state.outcomes[txHash]
                )
            }
            indexedUpTo = Int64(block.number)
            indexedHash = block.hash
        }

        return canonicalIndex
    }
}

// MARK: - Constants

private extension CoinageFuzzDriver {
    var seedCoins: [CoinageKeyIndex] { [1, 2, 3] }
    var seedVouchers: [CoinageKeyIndex] { [7, 8] }
    /// Wide enough that a walk never runs out: an index is offered once and never returned.
    var mintableVouchers: [CoinageKeyIndex] { (9 ... 128).map(CoinageKeyIndex.harness) }
    var voucherPool: [CoinageKeyIndex] { seedVouchers + mintableVouchers }
    var rings: [Int] { [5, 6] }
}

/// Short on purpose: at the production window of 128 blocks a walk never produces enough blocks for
/// mortality to expire, so every rule that turns on a closed window would go unexercised.
private let fuzzMortalPeriod: UInt32 = 12

/// The first coin index handed out for outputs, above the seed coins.
private let firstOutputCoin: CoinageKeyIndex = 100

/// One pass is not a fixpoint: an entry demoted by Rule 0 spends that pass being demoted and is decided
/// by the next. Two is the whole cascade for an expired entry.
private let passesToDecide = 2

/// Where a transaction landed on the canonical chain, and what its dispatch did there.
struct CanonicalInclusion {
    let blockNumber: UInt32
    let outcome: Bool?

    func succeededBelow(_ finalized: UInt32) -> Bool { blockNumber <= finalized && outcome == true }
    func failedBelow(_ finalized: UInt32) -> Bool { blockNumber <= finalized && outcome == false }
}

/// The frozen pair a terminal entry must never change.
private struct TerminalVerdict: Equatable {
    let status: CoinageTxStatus
    let successDetectedAt: BlockRef?
}

enum FuzzHarnessError: Error {
    case noEntry(Int64)
}

// MARK: - Invariants

private extension CoinageFuzzDriver {
    func checkInvariants(_ trace: [FuzzAction]) async throws {
        let entries = try await harness.store.getAllEntries()
        let canonical = canonicalTransactions()
        let finalized = harness.chain.finalizedHead.number
        let cleanPass = trace.last == .runPass

        for entry in entries {
            let inclusion = canonical[entry.submittedAttempt.txHash]

            try terminalVerdictIsFrozen(entry, trace)
            try failureIsJustified(entry, inclusion, finalized, trace)
            try successRestsOnACanonicalDispatch(entry, inclusion, finalized, trace)
            try failedEntryMintedNothing(entry, trace)
            try undecidedVerdictCarriesNoRecord(entry, trace)
            if cleanPass {
                try recordReferencesCanonicalBlockForLiveEntries(entry, trace)
                try expiredEntriesAreDecided(entry, trace)
                try finalizedSuccessIsNotWithheld(entry, inclusion, finalized, trace)
            }
        }

        try await handoffMarksAreHonoured(entries, trace)
        try everyAssetHasOneLiveClaimant(entries, trace)
    }

    /// The spec freezes the verdict, and the verdict is the pair: a terminal entry whose record changed
    /// underneath would leave its outputs' selectability resting on something new.
    func terminalVerdictIsFrozen(_ entry: CoinageTxEntry, _ trace: [FuzzAction]) throws {
        let verdict = TerminalVerdict(status: entry.status, successDetectedAt: entry.successDetectedAt)

        if let previous = terminalSeen[entry.sequence] {
            try require(verdict == previous, trace) {
                "entry \(entry.sequence) changed from terminal \(previous) to \(verdict)"
            }
        }
        if !entry.status.isLive { terminalSeen[entry.sequence] = verdict }
    }

    /// `failure` is terminal and hands the user back an input the chain may already have consumed, so it
    /// needs finalized evidence: a dispatch that failed at or below the finalized head, or a window
    /// closed there for a transaction that had not already succeeded under it.
    func failureIsJustified(
        _ entry: CoinageTxEntry,
        _ inclusion: CanonicalInclusion?,
        _ finalized: UInt32,
        _ trace: [FuzzAction]
    ) throws {
        guard entry.status == .failure else { return }

        let canNoLongerRunAndNeverDid = windowClosed(entry) && inclusion?.succeededBelow(finalized) != true

        try require(inclusion?.failedBelow(finalized) == true || canNoLongerRunAndNeverDid, trace) {
            "entry \(entry.sequence) was failed with nothing final to justify it: no dispatch failed " +
                "below the finalized head, and it either still has time to execute or has already succeeded"
        }
    }

    /// A finalized success means the transaction really is in a canonical block at or below the
    /// finalized head and dispatched successfully there — nothing weaker.
    func successRestsOnACanonicalDispatch(
        _ entry: CoinageTxEntry,
        _ inclusion: CanonicalInclusion?,
        _ finalized: UInt32,
        _ trace: [FuzzAction]
    ) throws {
        guard entry.status == .finalizedSuccess else { return }

        try require(inclusion?.succeededBelow(finalized) == true, trace) {
            "entry \(entry.sequence) is finalized but no canonical block at or below the finalized head " +
                "carries it with a successful dispatch"
        }
    }

    /// A failed entry never ran, so nothing it would have minted can be on the chain. If one is, the
    /// ledger has tombstoned a coin the chain still holds.
    func failedEntryMintedNothing(_ entry: CoinageTxEntry, _ trace: [FuzzAction]) throws {
        guard entry.status == .failure else { return }

        let finalizedCoins = coinKeysAtFinalizedHead()
        let present = entry.outputs.filter { finalizedCoins.contains($0.publicKey) }

        try require(present.isEmpty, trace) {
            "entry \(entry.sequence) was failed but the finalized chain still holds \(present.count) of its outputs"
        }
    }

    /// The record is what keeps outputs optimistically spendable, so a verdict claiming nothing carries
    /// nothing.
    func undecidedVerdictCarriesNoRecord(_ entry: CoinageTxEntry, _ trace: [FuzzAction]) throws {
        guard entry.status == .pending || entry.status == .failure else { return }

        try require(entry.successDetectedAt == nil, trace) {
            "entry \(entry.sequence) is \(entry.status) but still carries a detected-success record"
        }
    }

    func recordReferencesCanonicalBlockForLiveEntries(_ entry: CoinageTxEntry, _ trace: [FuzzAction]) throws {
        // Terminal entries may store dangling blocks; only live ones must reference a canonical block.
        guard entry.status.isLive, let recorded = entry.successDetectedAt else { return }

        let canonicalHash = harness.chain.canonicalAt(recorded.number)?.hash

        try require(canonicalHash == recorded.hash, trace) {
            "entry \(entry.sequence) kept a record of block \(recorded.number) across a pass, but that " +
                "block is not the canonical one"
        }
    }

    /// Liveness, which nothing else here checks: past its window a pass with no failing read has what it
    /// needs to decide an entry — the body search covers the whole era — so it must not stay `pending`.
    func expiredEntriesAreDecided(_ entry: CoinageTxEntry, _ trace: [FuzzAction]) throws {
        guard windowClosed(entry) else { return }

        let passes = (expiredPasses[entry.sequence] ?? 0) + 1
        expiredPasses[entry.sequence] = passes

        try require(entry.status != .pending || passes < passesToDecide, trace) {
            "entry \(entry.sequence) is still pending after \(passes) clean passes past its window, which " +
                "closed at \(UInt64(entry.submittedAttempt.checkpoint.number) + UInt64(entry.submittedAttempt.mortalityBlocks)) with the finalized " +
                "head at \(harness.chain.finalizedHead.number)"
        }
    }

    /// Once the finalized head reaches the block a transaction succeeded in, nothing about it can be
    /// rewritten, so a `pendingSuccess` entry must stop being pending.
    func finalizedSuccessIsNotWithheld(
        _ entry: CoinageTxEntry,
        _ inclusion: CanonicalInclusion?,
        _ finalized: UInt32,
        _ trace: [FuzzAction]
    ) throws {
        guard inclusion?.succeededBelow(finalized) == true else { return }

        let passes = (settledPasses[entry.sequence] ?? 0) + 1
        settledPasses[entry.sequence] = passes

        try require(entry.status == .finalizedSuccess || passes < passesToDecide, trace) {
            "entry \(entry.sequence) is \(entry.status) after \(passes) clean passes, though its " +
                "transaction succeeded in block \(inclusion?.blockNumber ?? 0), below the finalized head at \(finalized)"
        }
    }

    /// A key that has left for a peer can be spent by them at any moment, so nothing of ours may claim
    /// it, and a relaunch must not release a mark whose keys really did leave.
    func handoffMarksAreHonoured(_ entries: [CoinageTxEntry], _ trace: [FuzzAction]) async throws {
        let marks = try await harness.store.getHandoffKeys()

        for entry in entries where entry.status != .failure {
            try require(entry.inputs.allSatisfy { !marks.contains($0.publicKey) }, trace) {
                "entry \(entry.sequence) claims an asset that was handed off to a peer"
            }
        }
        try require(committedMarks.isSubset(of: marks), trace) {
            "a committed handoff mark did not survive: \(committedMarks.subtracting(marks))"
        }
    }

    func everyAssetHasOneLiveClaimant(_ entries: [CoinageTxEntry], _ trace: [FuzzAction]) throws {
        let claims = entries.filter { $0.status != .failure }.flatMap { entry in entry.inputs.map(\.publicKey) }

        try require(claims.count == Set(claims).count, trace) {
            "an asset is claimed by more than one entry that is not a failure"
        }
    }

    func require(_ condition: Bool, _ trace: [FuzzAction], _ message: () -> String) throws {
        if !condition { throw FuzzViolation(message: message(), trace: trace) }
    }
}

// MARK: - Seed

extension DurabilityHarness {
    /// The app starts holding coins and vouchers it did not mint itself, which is what a top-up looks
    /// like.
    func givenFuzzSeedAssets() {
        mintCoinsOnChain([1, 2, 3], finality: .finalized)
        for voucher in [CoinageKeyIndex(7), CoinageKeyIndex(8)] {
            givenVoucherInRecycler(voucher, denomination: 3, ring: 5, finality: .finalized)
        }
    }
}
