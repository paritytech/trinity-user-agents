import DurableTransactionsTestSupport
import Foundation
@testable import Coinage

/// Ring placement of a voucher, mirroring `MembersPallet.RingPosition`: only `.included` carries a
/// ring index, so — exactly as the production `VoucherOnChainQueryService` — only an included voucher
/// reads as present on chain. `onboarding`/`suspended` have no ring index and therefore read absent.
enum FakeRingPosition: Equatable {
    case included(ringIndex: Int)
    case onboarding
    case suspended

    var ringIndex: Int? {
        if case let .included(index) = self { return index }
        return nil
    }
}

/// The recycler-alias storage key, mirroring the production `RecyclerAliasStateKey`
/// (exponent, ringIndex, alias public key). Keying by the full triple — rather than the alias bytes
/// alone — is what lets a scenario write an unload under the *wrong* ring and have nothing read it.
struct FakeAliasKey: Hashable {
    let exponent: Int
    let ringIndex: Int
    let aliasPublicKey: Data
}

/// One on-chain coin row. `value`/`age` are unused by the durability engine (which reads only
/// presence) but kept so scenarios can model them where it aids readability.
struct FakeCoinInfo: Equatable {
    var value: Int
    var age: Int
}

/// What the chain holds at one block: which coins exist, which vouchers are recycler members and
/// where they sit in a ring, which aliases read as unloaded, and the dispatch outcome of every
/// extrinsic applied in that block.
///
/// `outcomes` is per-block rather than cumulative, so a transaction reorged out of one block and
/// re-applied in another carries the outcome of the block it is read at.
struct CoinageChainState: FakeChainState {
    var coins: [PublicKey: FakeCoinInfo]
    /// `RecyclersCoinToRecycler`: which denomination's recycler a voucher member belongs to.
    var recyclerMembers: [PublicKey: Int]
    /// `Members`: where a voucher member sits in that collection.
    var ringPositions: [PublicKey: FakeRingPosition]
    /// `RecyclerAliasStates`: presence (value `true`) means the alias read as unloaded.
    var aliases: [FakeAliasKey: Bool]
    /// Dispatch outcome of each extrinsic applied in this block: `true` success, `false` failure.
    var outcomes: [Data: Bool]

    static let empty = CoinageChainState(
        coins: [:],
        recyclerMembers: [:],
        ringPositions: [:],
        aliases: [:],
        outcomes: [:]
    )
}

/// The chain and block types the coinage harness drives.
typealias CoinageFakeChain = FakeChain<CoinageChainState>
typealias FakeBlock = DurableTransactionsTestSupport.FakeBlock<CoinageChainState>

// MARK: - Mutations

extension CoinageChainState {
    func mintCoin(_ key: PublicKey, value: Int = 1, age: Int = 0) -> Self {
        var next = self
        next.coins[key] = FakeCoinInfo(value: value, age: age)
        return next
    }

    func consumeCoin(_ key: PublicKey) -> Self {
        var next = self
        next.coins.removeValue(forKey: key)
        return next
    }

    func joinRecycler(member: PublicKey, exponent: Int, position: FakeRingPosition) -> Self {
        var next = self
        next.recyclerMembers[member] = exponent
        next.ringPositions[member] = position
        return next
    }

    /// Archival: the member-to-denomination entry and its ring position both go — the only thing that
    /// takes a voucher out of a recycler.
    func leaveRecycler(member: PublicKey) -> Self {
        var next = self
        next.recyclerMembers.removeValue(forKey: member)
        next.ringPositions.removeValue(forKey: member)
        return next
    }

    func withAlias(_ key: FakeAliasKey, unloaded: Bool = true) -> Self {
        var next = self
        next.aliases[key] = unloaded
        return next
    }

    func clearAlias(_ key: FakeAliasKey) -> Self {
        var next = self
        next.aliases.removeValue(forKey: key)
        return next
    }

    func applied(_ txHash: Data, success: Bool) -> Self {
        var next = self
        next.outcomes[txHash] = success
        return next
    }
}

// MARK: - Faults

/// Which coinage reads fail, so a scenario can hold an entry undecided without changing what the chain
/// holds. The engine-shaped faults (blocks, outcomes, pinning, the body search) live on the fake pinned
/// view factory's ``FakeChainFaults``.
///
/// Failures are per-key and per-block rather than global, because the spec distinguishes a pass that
/// reads nothing from one that reads only part of its window.
struct CoinageReadFaults {
    /// Coin keys that fail at every head.
    var unreadableCoins: Set<PublicKey> = []
    var unreadableAliases: Set<FakeAliasKey> = []
    var membershipsUnreadable = false
    var ringPositionsUnreadable = false
    /// Coin reads fail at these block *hashes* only. Separate from `unreadableCoins`, which fails a
    /// key at every head: the rules read the same asset at the finalized and the best head, and some
    /// turn on the two answers differing.
    var statelessBlocks: Set<Data> = []

    static let none = CoinageReadFaults()
}

/// A read that fails for the length of one pass. Every one leaves the ledger a state it cannot judge,
/// which is the point: the rules withhold a verdict rather than inventing one.
enum FuzzFault: CaseIterable {
    case coins
    case aliases
    case memberships
    case ringPositions
    case blocks
    case outcomes
    case pin
}
