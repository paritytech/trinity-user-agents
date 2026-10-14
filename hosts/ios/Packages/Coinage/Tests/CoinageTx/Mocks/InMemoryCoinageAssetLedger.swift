import AsyncExtensions
import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import os
import SubstrateSdk
@testable import Coinage

/// Coinage's half of the ledger in memory, over an ``InMemoryDurableTxRepository``: asset rows keyed by
/// the engine's ids, the four registration invariants, and the handoff marks.
///
/// Registration writes only inside the engine's scope, the way the CoreData ledger does; the invariants
/// run before any row is written, so a rejected batch leaves nothing behind here while the engine rolls
/// back its own rows.
final class InMemoryCoinageAssetLedger: CoinageAssetLedgerProtocol, @unchecked Sendable {
    let durable: InMemoryDurableTxRepository

    private struct State {
        var assets: [CoinageTxId: CoinageAssetRegistration] = [:]
        var pendingMarks: Set<OwnAsset> = []
        var committedMarks: Set<OwnAsset> = []
    }

    private let state = OSAllocatedUnfairLock(initialState: State())
    private let validator = CoinageTxRegistrationValidator()

    init(durable: InMemoryDurableTxRepository) {
        self.durable = durable
    }

    var handoffMarks: Set<OwnAsset> {
        state.withLock { $0.pendingMarks.union($0.committedMarks) }
    }

    /// Runs the four invariants against the current rows, the way registration does, without writing.
    func validate(_ registrations: [CoinageAssetRegistration]) throws {
        try validator.validate(registrations, transaction: validationContext())
    }

    /// Records the assets of an entry the durable store already holds — the test path that keeps a
    /// prepared entry's id.
    func storeAssets(_ assets: CoinageAssetRegistration, for id: CoinageTxId) {
        state.withLock { $0.assets[id] = assets }
    }

    /// Test helper: directly records a committed handoff mark (skips the two-phase flow).
    func markHandedOff(_ asset: OwnAsset) {
        state.withLock { _ = $0.committedMarks.insert(asset) }
    }

    // MARK: - CoinageAssetLedgerProtocol

    func registerAssets(
        _ registrations: [CoinageAssetRegistration],
        for ids: [CoinageTxId],
        in scope: any DurableTxRegistrationScope
    ) throws {
        guard scope is InMemoryRegistrationScope else {
            throw DurableTxError.foreignRegistrationScope
        }
        try validate(registrations)
        state.withLock { current in
            for (id, assets) in zip(ids, registrations) {
                current.assets[id] = assets
            }
        }
    }

    func getAllEntries() async throws -> [CoinageTxEntry] {
        joined(durable.allEntries)
    }

    func getEntry(id: CoinageTxId) async throws -> CoinageTxEntry? {
        guard let entry = try await durable.getEntry(id: id) else { return nil }
        return joined([entry]).first
    }

    func releaseUncommittedHandoffs(_ keys: [PublicKey]) async throws {
        let dropped = Set(keys)

        state.withLock { current in
            for asset in current.pendingMarks where dropped.contains(asset.publicKey) {
                current.pendingMarks.remove(asset)
            }
        }
    }

    func commitHandoffs(_ keys: [PublicKey], in _: any DurableTxRegistrationScope) throws {
        let keySet = Set(keys)

        state.withLock { current in
            for asset in current.pendingMarks where keySet.contains(asset.publicKey) {
                current.pendingMarks.remove(asset)
                current.committedMarks.insert(asset)
            }
        }
    }

    func assets(of ids: [CoinageTxId]) async throws -> [CoinageTxId: CoinageTxEntry] {
        let wanted = Set(ids)

        return try await getAllEntries()
            .filter { wanted.contains($0.id) }
            .reduce(into: [:]) { $0[$1.id] = $1 }
    }

    func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry] {
        try await joined(durable.getGroupEntries(domain: .coinage, groupId: groupId))
    }

    func subscribeOperationGroupStatuses(_ groupId: CoinageTxGroupId) -> AnyAsyncSequence<[CoinageTxEntry]> {
        durable.subscribeGroupEntries(domain: .coinage, groupId: groupId)
            .map { [self] entries in joined(entries) }
            .eraseToAnyAsyncSequence()
    }

    /// Mirrors the CoreData ledger: a coin already carrying a mark is refused rather than skipped.
    /// Tolerating the re-mark let the suite pass on ``CoinageTxService``'s own `filterHandedOff`
    /// check alone, leaving the ledger's guard — the one inside the write transaction, where it
    /// actually has to hold — uncovered.
    func precommitHandOff(
        _ assets: [OwnAsset],
        validation: @escaping (any CoinageTxValidationContextProtocol) throws -> Void
    ) async throws {
        try validation(validationContext())
        try state.withLock { current in
            for asset in assets where current.pendingMarks.contains(asset)
                || current.committedMarks.contains(asset) {
                throw CoinageTxError.handoffOfHandedOffAsset(asset.publicKey.toHex())
            }

            for asset in assets {
                current.pendingMarks.insert(asset)
            }
        }
    }

    func releaseUncommittedHandoffs() async throws {
        state.withLock { $0.pendingMarks.removeAll() }
    }

    func handedOffCoins() async throws -> [OwnAsset] {
        Array(handoffMarks)
    }
}

private extension InMemoryCoinageAssetLedger {
    func joined(_ entries: [DurableTxEntry]) -> [CoinageTxEntry] {
        let assets = state.withLock { $0.assets }
        return entries.compactMap { entry in
            guard entry.domainId == .coinage, let registration = assets[entry.id] else { return nil }
            return CoinageTxEntry(entry: entry, inputs: registration.inputs, outputs: registration.outputs)
        }
    }

    func validationContext() -> InMemoryValidationContext {
        let assets = state.withLock { $0.assets }
        let statuses = Dictionary(uniqueKeysWithValues: assets.keys.map { ($0, durable.statusSnapshot(of: $0)) })
        return InMemoryValidationContext(
            assets: assets,
            statuses: statuses,
            handedOff: Set(handoffMarks.map(\.publicKey))
        )
    }
}

/// The four public-key-keyed reads over the in-memory rows, the counterpart of the CoreData context.
private struct InMemoryValidationContext: CoinageTxValidationContextProtocol {
    let assets: [CoinageTxId: CoinageAssetRegistration]
    let statuses: [CoinageTxId: DurableTxStatus?]
    let handedOff: Set<PublicKey>

    func filterMinted(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        let minted = Set(assets.values.flatMap { $0.outputs.map(\.publicKey) })
        return keys.intersection(minted)
    }

    func filterReceived(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        var received: Set<PublicKey> = []
        for registration in assets.values {
            for input in registration.inputs {
                if case let .coin(.received(key)) = input { received.insert(key) }
            }
        }
        return keys.intersection(received)
    }

    func filterClaimed(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        let claimed = Set(
            assets
                .filter { id, _ in statuses[id] != .failure }
                .flatMap { _, registration in registration.inputs.map(\.publicKey) }
        )
        return keys.intersection(claimed)
    }

    func filterHandedOff(_ keys: Set<PublicKey>) throws -> Set<PublicKey> {
        keys.intersection(handedOff)
    }
}
