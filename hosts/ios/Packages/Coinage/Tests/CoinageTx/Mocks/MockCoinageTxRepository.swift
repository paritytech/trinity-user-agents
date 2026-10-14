import AsyncExtensions
import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import SubstrateSdk
@testable import Coinage

/// The two halves of the ledger the durability suites read and write: the engine's in-memory
/// repository and coinage's in-memory asset ledger, behind the one-object API the suites were written
/// against.
final class MockCoinageTxRepository: @unchecked Sendable {
    let durable: InMemoryDurableTxRepository
    let ledger: InMemoryCoinageAssetLedger

    init() {
        durable = InMemoryDurableTxRepository()
        ledger = InMemoryCoinageAssetLedger(durable: durable)
    }

    var allEntries: [CoinageTxEntry] {
        get async throws { try await ledger.getAllEntries() }
    }

    var handoffMarks: Set<OwnAsset> {
        ledger.handoffMarks
    }

    /// Registers a prepared entry directly, keeping its id and status — the many suites that arrange a
    /// ledger state without going through submission. Enforces the invariants the way registration does.
    func register(_ entry: CoinageTxEntry) async throws {
        let assets = CoinageAssetRegistration(inputs: entry.inputs, outputs: entry.outputs)
        try ledger.validate([assets])
        durable.insert(entry.entry)
        ledger.storeAssets(assets, for: entry.id)
    }

    /// Test-only convenience (the production protocol has only the compare-and-set `updateTxStatus`):
    /// forces a status and notifies observers, for setting up scenarios.
    func updateStatus(_ id: CoinageTxId, to status: CoinageTxStatus) async throws {
        try durable.forceStatus(id, to: status)
    }

    @discardableResult
    func updateTxStatus(
        for id: CoinageTxId,
        expectedCurrentStatus: CoinageTxStatus,
        expectedTxHash: Data,
        verdict: Verdict
    ) async throws -> Bool {
        try await durable.updateTxStatus(
            for: id,
            expectedCurrentStatus: expectedCurrentStatus,
            expectedTxHash: expectedTxHash,
            verdict: verdict
        )
    }

    func getAllEntries() async throws -> [CoinageTxEntry] {
        try await ledger.getAllEntries()
    }

    func getEntry(id: CoinageTxId) async throws -> CoinageTxEntry? {
        try await ledger.getEntry(id: id)
    }

    func getStatus(_ id: CoinageTxId) async throws -> CoinageTxStatus? {
        try await ledger.getStatus(id)
    }

    func subscribeStatus(id: CoinageTxId) -> AnyAsyncSequence<CoinageTxStatus> {
        durable.subscribeStatus(id: id)
    }

    func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry] {
        try await ledger.getOperationGroupStatuses(groupId)
    }

    func subscribeOperationGroupStatuses(_ groupId: CoinageTxGroupId) -> AnyAsyncSequence<[CoinageTxEntry]> {
        ledger.subscribeOperationGroupStatuses(groupId)
    }

    func minter(of asset: OwnAsset) async throws -> CoinageTxEntry? {
        try await ledger.minter(of: asset)
    }

    func consumers(of input: CoinageTxInput) async throws -> [CoinageTxEntry] {
        try await ledger.consumers(of: input)
    }

    /// Test helper: directly records a committed handoff mark (skips the two-phase flow).
    func markHandedOff(_ asset: OwnAsset) async throws {
        ledger.markHandedOff(asset)
    }

    func precommitHandOff(
        _ assets: [OwnAsset],
        validation: @escaping (any CoinageTxValidationContextProtocol) throws -> Void
    ) async throws {
        try await ledger.precommitHandOff(assets, validation: validation)
    }

    func releaseUncommittedHandoffs() async throws {
        try await ledger.releaseUncommittedHandoffs()
    }

    func handedOffCoins() async throws -> [OwnAsset] {
        try await ledger.handedOffCoins()
    }

    func getHandoffKeys() async throws -> Set<PublicKey> {
        try await ledger.getHandoffKeys()
    }
}
