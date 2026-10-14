import Foundation
import SDKLogger

/// The only thing that adds transactions to the ledger.
///
/// Hands registrations to the repository, which inserts them in one transaction and runs the domain's
/// hook inside it — so a rejected registration leaves nothing behind, in either store. Ownership is
/// taken inside that same transaction, so a pass can never reach a committed entry before its watcher.
public struct DurableTxRegistrar: Sendable {
    private let store: any DurableTxRepositoryProtocol
    private let owned: DurableTxOwnershipSet
    private let logger: SDKLoggerProtocol?

    public init(
        store: any DurableTxRepositoryProtocol,
        owned: DurableTxOwnershipSet,
        logger: SDKLoggerProtocol? = nil
    ) {
        self.store = store
        self.owned = owned
        self.logger = logger
    }

    /// Registers every registration atomically — all commit or none do — running `onRegister` inside the
    /// transaction with the minted ids, and takes ownership of each attempt. Returns the ids in
    /// registration order.
    public func register(
        _ registrations: [DurableTxRegistration],
        onRegister: @escaping DurableTxRegistrationHook
    ) async throws -> [DurableTxId] {
        guard !registrations.isEmpty else { return [] }

        let hashes = registrations.map(\.attempt.txHash)
        let taken = OwnedAttempts()

        do {
            let ids = try await store.register(registrations) { [owned] scope, ids in
                try onRegister(scope, ids)
                taken.take(zip(ids, hashes).map { ($0, $1) }, into: owned)
            }
            logger?.debug("Registered \(ids.count) durable transaction(s)")

            return ids
        } catch {
            // If ownership was taken but the transaction then rolled back, hand it back.
            taken.releaseAll(from: owned)

            throw error
        }
    }
}

/// Collects the attempts the hook took ownership of, so ownership can be handed back if the transaction
/// rolls back after the hook ran. A reference box: the hook runs on the store's queue, and the enclosing
/// `await` orders that write before the read here.
private final class OwnedAttempts: @unchecked Sendable {
    private(set) var attempts: [(DurableTxId, Data)] = []

    func take(_ attempts: [(DurableTxId, Data)], into owned: DurableTxOwnershipSet) {
        self.attempts = attempts
        attempts.forEach { owned.take($0.0, txHash: $0.1) }
    }

    func releaseAll(from owned: DurableTxOwnershipSet) {
        attempts.forEach { owned.release($0.0, txHash: $0.1) }
    }
}
