import AsyncExtensions
import Coinage
import DurableTransactions
import Foundation

@testable import polkadot_app

/// The two CoreData halves of the ledger wired the way the app wires them — the engine's store opening
/// the transaction, coinage's asset ledger writing inside it — behind the registration shape the
/// durability suites use.
struct CoinageCoreDataLedger {
    let durable: DurableTxCoreDataRepository
    let ledger: CoinageAssetLedgerCoreData

    init(storageFacade: StorageFacadeProtocol) {
        durable = DurableTxCoreDataRepository(storageFacade: storageFacade, rowObservers: [CoinageTxRowObserver()])
        ledger = CoinageAssetLedgerCoreData(storageFacade: storageFacade)
    }

    /// Registers the batch the production way: engine rows and asset rows in one transaction, the real
    /// invariant checks inside it. Returns the ids the store minted.
    @discardableResult
    func register(_ registrations: [CoinageTxRegistration]) async throws -> [CoinageTxId] {
        let assets = registrations.map(\.assets)
        return try await durable.register(registrations.map(\.durable)) { scope, ids in
            try ledger.registerAssets(assets, for: ids, in: scope)
        }
    }

    /// Registers a prepared entry, keeping the `store.register(entry)` call shape the suites were written
    /// against; the id is the one the store mints, not the entry's.
    @discardableResult
    func register(_ entry: CoinageTxEntry) async throws -> CoinageTxId {
        // These suites register rows that were built, so a missing attempt is a broken fixture.
        guard let attempt = entry.attempt else {
            throw CoinageTxError.entryNotFound(entry.id)
        }

        let registration = CoinageTxRegistration(
            txHash: attempt.txHash,
            checkpoint: attempt.checkpoint,
            mortalityBlocks: attempt.mortalityBlocks,
            groupId: entry.groupId,
            inputs: entry.inputs,
            outputs: entry.outputs
        )
        let ids = try await register([registration])
        guard let id = ids.first else { throw CoinageTxError.entryNotFound(entry.id) }
        return id
    }

    /// Reads the current status and applies a verdict through the production compare-and-set.
    func updateStatus(_ id: CoinageTxId, to status: CoinageTxStatus) async throws {
        guard let current = try await durable.getEntry(id: id) else {
            throw CoinageTxError.entryNotFound(id)
        }
        guard let attempt = current.attempt else {
            throw CoinageTxError.entryNotFound(id)
        }

        _ = try await durable.updateTxStatus(
            for: id,
            expectedCurrentStatus: current.status,
            expectedTxHash: attempt.txHash,
            verdict: Verdict(status: status, successDetectedAt: nil)
        )
    }

    func getAllEntries() async throws -> [CoinageTxEntry] {
        try await ledger.getAllEntries()
    }

    func handedOffCoins() async throws -> [OwnAsset] {
        try await ledger.handedOffCoins()
    }

    func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry] {
        try await ledger.getOperationGroupStatuses(groupId)
    }

    func subscribeOperationGroupStatuses(_ groupId: CoinageTxGroupId) -> AnyAsyncSequence<[CoinageTxEntry]> {
        ledger.subscribeOperationGroupStatuses(groupId)
    }
}
