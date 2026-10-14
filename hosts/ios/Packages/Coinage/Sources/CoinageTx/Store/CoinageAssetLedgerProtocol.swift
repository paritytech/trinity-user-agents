import AsyncExtensions
import DurableTransactions
import Foundation

/// Coinage's half of the ledger: which assets each transaction consumes and mints, and which carry a
/// handoff lock.
///
/// The transaction rows themselves belong to the durability engine. These rows key on the id it assigns
/// and are written inside its registration transaction, so the two commit together. Entries are never
/// deleted; terminal rows stay as history because `minter(of:)` and `consumers(of:)` must still see them.
public protocol CoinageAssetLedgerProtocol: Sendable {
    /// Writes the asset rows for `registrations` (one per id, in order) after checking the four
    /// invariants they must not break.
    ///
    /// Only correct inside the engine's write transaction, which is what `scope` proves: throwing rolls
    /// the whole registration back, in both stores. A scope opened by another store technology must be
    /// refused with ``DurableTxError/foreignRegistrationScope``.
    func registerAssets(
        _ registrations: [CoinageAssetRegistration],
        for ids: [CoinageTxId],
        in scope: any DurableTxRegistrationScope
    ) throws

    /// Every entry, live and terminal, joined to its assets, ordered by `sequence`.
    func getAllEntries() async throws -> [CoinageTxEntry]

    /// The entry with this id joined to its assets, if any.
    func getEntry(id: CoinageTxId) async throws -> CoinageTxEntry?

    /// The entries of each of `ids`, in one batched read rather than one per transaction.
    ///
    /// A submission policy reads every transaction it was handed at once; the ledger is append-only and
    /// grows for the life of the installation, so this must not be answered by scanning all of it.
    func assets(of ids: [CoinageTxId]) async throws -> [CoinageTxId: CoinageTxEntry]

    /// Every entry registered under `groupId`, ordered by `sequence`.
    func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry]

    /// A stream of the entries registered under `groupId`: the current set, then every change, ordered
    /// by `sequence`.
    func subscribeOperationGroupStatuses(_ groupId: CoinageTxGroupId) -> AnyAsyncSequence<[CoinageTxEntry]>

    /// Provisionally marks assets handed off after running `validation` in the same transaction — so
    /// nothing can claim them between the check and the mark, and a throw rolls the whole thing back.
    /// Released on relaunch unless committed.
    func precommitHandOff(
        _ assets: [OwnAsset],
        validation: @escaping (any CoinageTxValidationContextProtocol) throws -> Void
    ) async throws

    /// Clears every uncommitted mark. Runs once, on launch.
    func releaseUncommittedHandoffs() async throws

    /// Drops the marks on `keys` that were never committed — the payment behind them never happened.
    func releaseUncommittedHandoffs(_ keys: [PublicKey]) async throws

    /// Promotes provisional marks to final — the keys have durably left.
    ///
    /// Only ever inside a transaction the caller already opened, so the marks become final exactly
    /// when whatever carries the keys does. Keyed by ``OwnAsset/publicKey``, the form the transport
    /// can name without reconstructing the asset.
    func commitHandoffs(_ keys: [PublicKey], in scope: any DurableTxRegistrationScope) throws

    func handedOffCoins() async throws -> [OwnAsset]
}

public extension CoinageAssetLedgerProtocol {
    /// Handoff marks as a set of ``OwnAsset/publicKey``, the form the DAG and callers compare against.
    func getHandoffKeys() async throws -> Set<PublicKey> {
        try await Set(handedOffCoins().map(\.publicKey))
    }

    /// The entry's current status, if it exists.
    func getStatus(_ id: CoinageTxId) async throws -> CoinageTxStatus? {
        try await getEntry(id: id)?.status
    }

    /// The entry that minted this asset, if any.
    func minter(of asset: OwnAsset) async throws -> CoinageTxEntry? {
        let key = asset.publicKey
        return try await getAllEntries().first { entry in
            entry.outputs.contains { $0.publicKey == key }
        }
    }

    /// Every entry consuming this input, including terminal ones.
    func consumers(of input: CoinageTxInput) async throws -> [CoinageTxEntry] {
        let key = input.publicKey
        return try await getAllEntries().filter { entry in
            entry.inputs.contains { $0.publicKey == key }
        }
    }
}
