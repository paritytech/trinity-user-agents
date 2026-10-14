import Operation_iOS

/// The two fields chain sync owns for a coin — on-chain `age` and `isOnchain` presence — keyed to
/// the coin by derivation index. Persisted through a dedicated write-only mapper so a presence
/// write never reads-then-overwrites the coin's other fields (handoff mark, etc.), which a peer
/// operation may have changed in between.
public struct CoinPresenceUpdate: Equatable, Sendable {
    public let derivationIndex: CoinageKeyIndex
    public let age: Int16?
    public let isOnchain: Bool

    public init(derivationIndex: CoinageKeyIndex, age: Int16?, isOnchain: Bool) {
        self.derivationIndex = derivationIndex
        self.age = age
        self.isOnchain = isOnchain
    }
}

extension CoinPresenceUpdate: Operation_iOS.Identifiable {
    public var identifier: String { Coin.identifier(for: derivationIndex) }
}
