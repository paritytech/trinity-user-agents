import DurableTransactions
import Foundation

/// A handoff that is reserved but not yet final.
///
/// Held from the moment the assets are chosen until whatever carries their keys is durable, then
/// committed. Where that is depends on the transport — for a chat payment it is the message row —
/// so the commit belongs inside the transaction that writes it: a crash in between would otherwise
/// clear the reservation while a peer already holds the keys.
///
/// Leaving a handle uncommitted is safe: a relaunch releases every provisional mark, returning the
/// coins. The only way for a mark to outlive the process is for the keys to have actually left.
public protocol CoinageHandoffCommit: Sendable {
    /// Commits inside the transaction the transport already opened, so the marks become final in the
    /// same write that makes the keys durable — the only placement where a crash cannot either strand
    /// the coins or release keys a peer already has. Synchronous because that caller is.
    func commit(in scope: any DurableTxRegistrationScope) throws

    /// Drops the reservation now, for a payment whose keys never left. A committed handoff is not
    /// touched. Without this the coins wait for a relaunch to be returned.
    func release() async throws
}

/// A ``CoinageHandoffCommit`` backed by the asset ledger: committing promotes the provisional marks on
/// `assets` to final.
struct StoreHandoffCommit: CoinageHandoffCommit {
    let assets: [OwnAsset]
    let ledger: any CoinageAssetLedgerProtocol

    func commit(in scope: any DurableTxRegistrationScope) throws {
        try ledger.commitHandoffs(assets.map(\.publicKey), in: scope)
    }

    func release() async throws {
        try await ledger.releaseUncommittedHandoffs(assets.map(\.publicKey))
    }
}
