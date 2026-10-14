import Foundation
import SubstrateSdk
import Operation_iOS

/// A coin currently residing in the Recycler, waiting for anonymity.
public struct Voucher: Equatable, CoinageDerivable, Sendable {
    public let exponent: Int16 // 2^n
    public let derivationIndex: CoinageKeyIndex
    public let allocatedAt: Date
    public let readyAt: Date
    public let remoteState: OnChainState

    /// Fungibility of the recycler holding this voucher, as a percentage in `0...100`.
    public let recyclerFungibility: UInt8

    /// The best fungibility this voucher's recycler can still reach, as a percentage in
    /// `0...100` — an upper bound on `recyclerFungibility` as the recycler keeps filling.
    public let maxRecyclerFungibility: UInt8

    /// On-chain public key (member key) derived from `derivationIndex`, cached so the durability
    /// layer never re-derives it on the fly.
    public let publicKey: PublicKey

    public var recycler: Recycler? { remoteState.recycler }

    public enum OnChainState: Equatable, Sendable {
        case unlocated
        case onboarding
        case inRecycler(Recycler)

        var recycler: Recycler? {
            switch self {
            case let .inRecycler(recycler): recycler
            case .unlocated,
                 .onboarding: nil
            }
        }

        public var pending: Bool {
            switch self {
            case .unlocated,
                 .onboarding: true
            case .inRecycler: false
            }
        }
    }

    public struct Recycler: Equatable, Sendable {
        public let index: UInt32
        public let membersCount: UInt32
        /// First locally confirmed inclusion in this ring.
        /// Nil until confirmed. Saved across restarts and reset on restore.
        public let enteredAt: Date?

        public init(index: UInt32, membersCount: UInt32, enteredAt: Date? = nil) {
            self.index = index
            self.membersCount = membersCount
            self.enteredAt = enteredAt
        }
    }

    public init(
        exponent: Int16,
        derivationIndex: CoinageKeyIndex,
        allocatedAt: Date,
        readyAt: Date,
        remoteState: OnChainState = .unlocated,
        // Zero until the chain assigns a ring: the index is not known when the voucher is minted,
        // so there is nothing to compute a score from, and zero reads as "no anonymity yet".
        recyclerFungibility: UInt8 = 0,
        maxRecyclerFungibility: UInt8 = 0,
        publicKey: PublicKey
    ) {
        self.exponent = exponent
        self.derivationIndex = derivationIndex
        self.allocatedAt = allocatedAt
        self.readyAt = readyAt
        self.remoteState = remoteState
        self.recyclerFungibility = recyclerFungibility
        self.maxRecyclerFungibility = maxRecyclerFungibility
        self.publicKey = publicKey
    }

    public func adjusting(state: OnChainState) -> Voucher {
        Voucher(
            exponent: exponent,
            derivationIndex: derivationIndex,
            allocatedAt: allocatedAt,
            readyAt: readyAt,
            remoteState: state,
            recyclerFungibility: recyclerFungibility,
            maxRecyclerFungibility: maxRecyclerFungibility,
            publicKey: publicKey
        )
    }

    public var isInRecycler: Bool {
        if case .inRecycler = remoteState { true } else { false }
    }
}

extension Voucher: Operation_iOS.Identifiable {
    public var identifier: String {
        Self.identifier(for: derivationIndex)
    }
}

public extension Voucher {
    /// The storage identifier for a voucher at `derivationIndex`. Single source of truth so no
    /// call site hand-writes the string form.
    static func identifier(for derivationIndex: CoinageKeyIndex) -> String {
        derivationIndex.toString()
    }
}
