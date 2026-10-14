import Foundation
import Operation_iOS
import SubstrateSdk

public struct Coin: Hashable, CoinageDerivable, Sendable {
    public let exponent: Int16 // 2^n
    public let derivationIndex: CoinageKeyIndex

    /// On-chain age. `nil` means the coin was never seen on chain; `0` is fresh from unload/split.
    public let age: Int16?

    /// On-chain presence, written only by chain sync. Combined with `age`, `age != nil ∧ ¬isOnchain`
    /// means the coin was seen on chain and has since vanished.
    public var isOnchain: Bool = false

    /// Whether the coin has been handed off to a peer, and how far along.
    public var handoffMark: CoinHandoffMark = .none

    /// Fungibility of the recycler this coin came out of, as a percentage in `0...100`.
    /// `nil` when it is not known — a coin recovered from chain state carries no record of it.
    public let recyclerFungibility: UInt8?

    /// Ordered provenance of this coin — the operations that produced it.
    public let hops: [Hop]

    /// On-chain public key (account id) derived from `derivationIndex`, cached so the durability
    /// layer never re-derives it on the fly.
    public let publicKey: PublicKey

    public init(
        exponent: Int16,
        derivationIndex: CoinageKeyIndex,
        age: Int16?,
        isOnchain: Bool = false,
        handoffMark: CoinHandoffMark = .none,
        recyclerFungibility: UInt8? = nil,
        hops: [Hop] = [],
        publicKey: PublicKey
    ) {
        self.exponent = exponent
        self.derivationIndex = derivationIndex
        self.age = age
        self.isOnchain = isOnchain
        self.handoffMark = handoffMark
        self.recyclerFungibility = recyclerFungibility
        self.hops = hops
        self.publicKey = publicKey
    }

    public func changing(age: Int16) -> Coin {
        Coin(
            exponent: exponent,
            derivationIndex: derivationIndex,
            age: age,
            isOnchain: isOnchain,
            handoffMark: handoffMark,
            recyclerFungibility: recyclerFungibility,
            hops: hops,
            publicKey: publicKey
        )
    }

    public func changing(isOnchain: Bool) -> Coin {
        Coin(
            exponent: exponent,
            derivationIndex: derivationIndex,
            age: age,
            isOnchain: isOnchain,
            handoffMark: handoffMark,
            recyclerFungibility: recyclerFungibility,
            hops: hops,
            publicKey: publicKey
        )
    }

    public func changing(handoffMark: CoinHandoffMark) -> Coin {
        Coin(
            exponent: exponent,
            derivationIndex: derivationIndex,
            age: age,
            isOnchain: isOnchain,
            handoffMark: handoffMark,
            recyclerFungibility: recyclerFungibility,
            hops: hops,
            publicKey: publicKey
        )
    }
}

extension Coin: Operation_iOS.Identifiable {
    public var identifier: String {
        Self.identifier(for: derivationIndex)
    }
}

public extension Coin {
    /// The storage identifier for a coin at `derivationIndex`. Single source of truth so no
    /// call site hand-writes the string form.
    static func identifier(for derivationIndex: CoinageKeyIndex) -> String {
        derivationIndex.toString()
    }

    var hasEverBeenOnChain: Bool {
        age != nil
    }

    /// Coins past `coinMaxAge` are due for imminent recycling and must not be
    /// picked for new transfers — the chain may invalidate them before inclusion.
    var isAgeValidToSpend: Bool {
        guard let age else { return false }
        return age < CoinageConstants.recycleAtAge
    }

    var ageOrDefault: Int16 {
        age ?? -1
    }
}
