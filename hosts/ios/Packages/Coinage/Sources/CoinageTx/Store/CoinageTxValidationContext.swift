import Foundation

/// The public-key-keyed reads a registration validates against, all inside one store transaction —
/// nothing it checks can move before the entry is written.
///
/// Every asset is identified by its on-chain public key: an own coin or voucher by the key derived
/// from its index, a coin received from a peer by the key itself. One key space covers them all,
/// which is why the four checks take and return `Set<PublicKey>`.
public protocol CoinageTxValidationContextProtocol {
    /// Of `keys`, those already minted as an output by any entry.
    func filterMinted(_ keys: Set<PublicKey>) throws -> Set<PublicKey>
    /// Of `keys`, those recorded as a received-coin input by any entry.
    func filterReceived(_ keys: Set<PublicKey>) throws -> Set<PublicKey>
    /// Of `keys`, those already claimed as an input by a non-failure entry.
    func filterClaimed(_ keys: Set<PublicKey>) throws -> Set<PublicKey>
    /// Of `keys`, those carrying a handoff mark.
    func filterHandedOff(_ keys: Set<PublicKey>) throws -> Set<PublicKey>
}
