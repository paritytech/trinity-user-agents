import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
@testable import Coinage

/// The projected state of one asset: who minted it, who consumes it, and whether it was handed off —
/// the durability read a scenario asserts against.
struct HarnessAssetState {
    let minterStatus: CoinageTxStatus?
    let consumerStatus: CoinageTxStatus?
    let handedOff: Bool
}

// MARK: - Reads

extension DurabilityHarness {
    func status(of id: CoinageTxId) async throws -> CoinageTxStatus? { try await store.getStatus(id) }

    func entry(_ id: CoinageTxId) async throws -> CoinageTxEntry? { try await store.getEntry(id: id) }

    func handoffKeys() async throws -> Set<PublicKey> { try await store.getHandoffKeys() }

    /// The per-submission status stream a watcher scenario drives (0-based submission index). A stream
    /// pushes chain statuses that ``releaseSubmissions()`` flushes through the real tracker.
    var submissionStatuses: (@Sendable (Int) -> HarnessStatusStream?)? {
        get { submitter.streamFactory }
        set { submitter.streamFactory = newValue }
    }

    /// How many extrinsics the tracker has submitted, so a scenario can assert no resubmission followed.
    var submissionCount: Int { submitter.submissionCount }

    /// The state of a coin as the ledger projects it. `consumerStatus` reports a non-failure consumer,
    /// since a failed spend releases its claim.
    func assetState(coin: CoinageKeyIndex) async throws -> HarnessAssetState {
        let minter = try await store.minter(of: coinOutput(coin))
        let consumers = try await store.consumers(of: coinInput(coin))
        let liveConsumer = consumers.first { $0.status != .failure }
        let marked = try await store.getHandoffKeys().contains(HarnessKeys.coinKey(coin))
        return HarnessAssetState(
            minterStatus: minter?.status,
            consumerStatus: liveConsumer?.status,
            handedOff: marked
        )
    }

    /// The state of a voucher as the ledger projects it. Vouchers are never handed off.
    func assetState(voucher: CoinageKeyIndex) async throws -> HarnessAssetState {
        let minter = try await store.minter(of: voucherOutput(voucher))
        let consumers = try await store.consumers(of: voucherInput(voucher))
        let liveConsumer = consumers.first { $0.status != .failure }
        return HarnessAssetState(
            minterStatus: minter?.status,
            consumerStatus: liveConsumer?.status,
            handedOff: false
        )
    }

    /// What the rules would read for `id` against a view pinned now — the real
    /// ``CoinageEvidenceCollector`` over the fake chain.
    func evidence(for id: CoinageTxId) async throws -> ChainEvidence {
        guard let entry = try await store.getEntry(id: id) else { throw FuzzHarnessError.noEntry(0) }
        let view = try await chainFactory.pin(chainId: harnessChainId)
        return await CoinageEvidenceCollector().collect(entry: entry, reader: stateReader, heads: view.heads)
    }
}

// MARK: - Chain evolution

extension DurabilityHarness {
    /// Applies `txHash` in a new block with the given outcome, returning that block — the block, not its
    /// number, so a scenario about the wrong block being read can hold on to the one it meant across a
    /// reorg.
    @discardableResult
    func includeInBlock(txHash: Data, success: Bool, finality: TestActionFinality) -> FakeBlock {
        chain.produceBlock(body: [txHash]) { $0.applied(txHash, success: success) }
        if finality == .finalized { finalizeToBest() }
        return chain.bestHead
    }

    /// Advances the best head past `id`'s mortality end — and the finalized head with it when asked — so
    /// the window closes. Read from the entry rather than a constant, so this keeps working if the window
    /// changes.
    func chainReachesMortalityOf(_ id: CoinageTxId, finality: TestActionFinality) async throws {
        guard let entry = try await store.getEntry(id: id) else { return }
        let target = entry.submittedAttempt.checkpoint.number + entry.submittedAttempt.mortalityBlocks + 1
        while chain.bestHead.number < target {
            chain.produceBlock()
        }
        if finality == .finalized { finalizeToBest() }
    }
}

// MARK: - Arranging entries

extension DurabilityHarness {
    /// Registered, and its watcher released — a pass decides only entries submission no longer owns.
    @discardableResult
    func givenUnwatchedEntry(inputCoin: CoinageKeyIndex, outputCoin: CoinageKeyIndex) async throws -> CoinageTxId {
        let id = try await register(inputCoin: inputCoin, outputCoin: outputCoin)
        await releaseSubmissions()
        return id
    }

    /// The transaction really executed: it sits in a block with a successful dispatch, its input consumed
    /// and its output minted. The ledger has not been told — the entry stays pending until a pass reads
    /// the chain.
    @discardableResult
    func givenEntryExecutedOnChain(
        inputCoin: CoinageKeyIndex,
        outputCoin: CoinageKeyIndex,
        finality: TestActionFinality
    ) async throws -> CoinageTxId {
        let id = try await givenUnwatchedEntry(inputCoin: inputCoin, outputCoin: outputCoin)
        guard let entry = try await store.getEntry(id: id) else { return id }

        produceBlock(finality, body: [entry.submittedAttempt.txHash]) { state in
            state
                .applied(entry.submittedAttempt.txHash, success: true)
                .consumeCoin(HarnessKeys.coinKey(inputCoin))
                .mintCoin(HarnessKeys.coinKey(outputCoin))
        }
        return id
    }

    /// The same, plus the pass that reads it, so the ledger has a verdict — `finalizedSuccess` when the
    /// block was finalized, `pendingSuccess` when it is only in the best chain. The rules derive that from
    /// `finality`; this helper does not choose it.
    @discardableResult
    func givenEntryDecided(
        inputCoin: CoinageKeyIndex,
        outputCoin: CoinageKeyIndex,
        finality: TestActionFinality
    ) async throws -> CoinageTxId {
        let id = try await givenEntryExecutedOnChain(inputCoin: inputCoin, outputCoin: outputCoin, finality: finality)
        await runPass()
        return id
    }
}

// MARK: - Faults

extension DurabilityHarness {
    /// Takes the body search out of play, so only the rules above it can decide an entry — every scenario
    /// whose subject is a rule opens with this, leaving the search on only where it is the subject.
    func disableFallbackTxSearch() {
        chainFactory.faults.txSearchDisabled = true
    }

    func makeCoinsUnreadable(_ coins: CoinageKeyIndex...) {
        stateReader.faults.unreadableCoins.formUnion(coins.map { HarnessKeys.coinKey($0) })
    }

    func makeBlocksUnreadable(_ blockNumbers: UInt32...) {
        chainFactory.faults.unreadableBlocks.formUnion(blockNumbers)
    }

    /// Silences the alias read of named vouchers — those in a ring, since one with no ring index has no
    /// alias key to fail.
    func makeVoucherAliasesUnreadable(_ vouchers: CoinageKeyIndex...) {
        for voucher in vouchers {
            if let key = currentAliasKey(index: voucher) {
                stateReader.faults.unreadableAliases.insert(key)
            }
        }
    }

    func makeRecyclerMembershipsUnreadable() {
        stateReader.faults.membershipsUnreadable = true
    }

    func makeRingPositionsUnreadable() {
        stateReader.faults.ringPositionsUnreadable = true
    }

    func makeEveryBlockUnreadable() {
        chainFactory.faults.everyBlockUnreadable = true
    }

    func makeChainUnreachable() {
        chainFactory.faults.pinFails = true
    }

    func clearFaults() {
        chainFactory.faults = .none
        stateReader.faults = .none
    }
}
