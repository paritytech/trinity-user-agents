import Foundation

/// The two facts about an asset that only the durability subsystem knows: the status of the entry
/// consuming it, and the status of the entry that minted it.
public struct CoinageAssetState: Equatable, Sendable {
    // whether a coin was transferred to another recipient
    public let handedOff: Bool

    /// The status of the non-failure entry consuming this asset, or nil if none does.
    public let consumerStatus: CoinageTxStatus?
    /// The status of the minting entry, or nil if no local entry minted this asset.
    /// Distinguishes absence before minting from absence after revert. An asset recovered from a
    /// previous installation reads `.finalizedSuccess`: recovery only saves what the finalized chain holds.
    public let minterStatus: CoinageTxStatus?

    public init(handedOff: Bool, consumerStatus: CoinageTxStatus?, minterStatus: CoinageTxStatus?) {
        self.handedOff = handedOff
        self.consumerStatus = consumerStatus
        self.minterStatus = minterStatus
    }
}

public extension CoinageAssetState {
    var isInUse: Bool { consumerStatus?.isLive == true }
    var isConsumed: Bool { consumerStatus == .finalizedSuccess }
    var isFree: Bool { !handedOff && !isInUse && !isConsumed }
    var isMintingFailed: Bool { minterStatus == .failure }
}
