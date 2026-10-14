import Foundation
import SubstrateSdk

/// A coin or voucher consumed by a transaction, carrying the on-chain public key it is keyed by.
public enum CoinageTxInput: Hashable, Sendable {
    case coin(CoinInput)
    case recyclerVoucher(CoinageKeyIndex, PublicKey)
}

/// A coin an entry consumes: either one this wallet minted, addressed by derivation index and its
/// derived public key, or one received from a peer, addressed by the on-chain public key alone.
public enum CoinInput: Hashable, Sendable {
    case own(CoinageKeyIndex, PublicKey)
    case received(PublicKey)
}

/// An asset this wallet mints, and can therefore hold local projected state for. Carries the
/// on-chain public key it is keyed by.
public enum OwnAsset: Hashable, Sendable {
    case coin(CoinageKeyIndex, PublicKey)
    case recyclerVoucher(CoinageKeyIndex, PublicKey)
}

/// An asset a durability entry mints. An output is always one this wallet owns.
public typealias CoinageTxOutput = OwnAsset

public extension OwnAsset {
    /// On-chain identity — the key dedup, handoff, and the DAG/evidence are all keyed by. Shared
    /// with ``CoinageTxInput/publicKey`` so an output and an input naming the same asset match.
    var publicKey: PublicKey {
        switch self {
        case let .coin(_, publicKey),
             let .recyclerVoucher(_, publicKey): publicKey
        }
    }

    var isCoin: Bool {
        if case .coin = self { return true }
        return false
    }

    /// Derivation index, which every own asset has.
    var derivationIndex: CoinageKeyIndex {
        switch self {
        case let .coin(index, _),
             let .recyclerVoucher(index, _): index
        }
    }

    var asInput: CoinageTxInput {
        switch self {
        case let .coin(index, publicKey): .coin(.own(index, publicKey))
        case let .recyclerVoucher(index, publicKey): .recyclerVoucher(index, publicKey)
        }
    }
}

public extension CoinageTxInput {
    /// On-chain identity — for a received coin the key itself, otherwise the key derived from the
    /// asset's index.
    var publicKey: PublicKey {
        switch self {
        case let .coin(.own(_, publicKey)),
             let .recyclerVoucher(_, publicKey): publicKey
        case let .coin(.received(publicKey)): publicKey
        }
    }

    var isCoin: Bool {
        if case .coin = self { return true }
        return false
    }

    /// True when this wallet minted the asset, so its provenance can be traced to an entry.
    var isOwn: Bool {
        ownAsset != nil
    }

    /// The own-asset view of this input, or `nil` for a coin received from a peer.
    var ownAsset: OwnAsset? {
        switch self {
        case let .coin(.own(index, publicKey)): .coin(index, publicKey)
        case let .recyclerVoucher(index, publicKey): .recyclerVoucher(index, publicKey)
        case .coin(.received): nil
        }
    }
}
