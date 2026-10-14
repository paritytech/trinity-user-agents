import DurableTransactions
import DurableTransactionsTestSupport
import Foundation
import SubstrateSdk
@testable import Coinage

/// Coinage's reads over a ``CoinageFakeChain``, standing in for the production ``CoinageStateReader``.
/// Every read honours the ``CoinageStateReading`` contract: a failure mode becomes `failedRead`, never
/// `absent`.
///
/// `faults` is mutable so a single-pass fault can be switched on, a pass run, then switched off — each
/// read consults the faults in force at read time.
final class FakeCoinageStateReader: CoinageStateReading, @unchecked Sendable {
    let chain: CoinageFakeChain
    var faults: CoinageReadFaults = .none

    init(chain: CoinageFakeChain) {
        self.chain = chain
    }

    func readInputs(_ inputs: [CoinageTxInput], at block: BlockRef) async -> [ReadResult<AssetPresence>] {
        inputs.map { read($0, at: block) }
    }

    func readOutputs(_ outputs: [OwnAsset], at block: BlockRef) async -> [ReadResult<AssetPresence>] {
        outputs.map { read($0.asInput, at: block) }
    }
}

// MARK: - One-asset resolution

private extension FakeCoinageStateReader {
    func read(_ input: CoinageTxInput, at block: BlockRef) -> ReadResult<AssetPresence> {
        guard let state = chain.stateAt(hash: block.hash) else { return .failedRead }

        if input.isCoin {
            return readCoin(input.publicKey, in: state, at: block)
        }
        guard case let .recyclerVoucher(index, memberKey) = input else { return .failedRead }
        return readVoucher(index: index, memberKey: memberKey, in: state)
    }

    func readCoin(_ key: PublicKey, in state: CoinageChainState, at block: BlockRef) -> ReadResult<AssetPresence> {
        if faults.unreadableCoins.contains(key) || faults.statelessBlocks.contains(block.hash) {
            return .failedRead
        }
        return state.coins[key] != nil ? .present(.coin) : .absent
    }

    /// Mirrors ``VoucherOnChainQueryService``: a recycler member is present whatever its ring position —
    /// Onboarding and Suspended included. A non-member (archival) reads `failedRead` (unknown), never
    /// absent, since a voucher's disappearance from the recycler is not consumption. `isUnloaded` is set
    /// only for a ring-placed voucher whose alias reads set; without a ring index there is no alias, so
    /// it reads not-unloaded.
    func readVoucher(
        index: CoinageKeyIndex,
        memberKey: PublicKey,
        in state: CoinageChainState
    ) -> ReadResult<AssetPresence> {
        if faults.membershipsUnreadable { return .failedRead }
        guard let exponent = state.recyclerMembers[memberKey] else { return .failedRead }

        if faults.ringPositionsUnreadable { return .failedRead }
        guard let position = state.ringPositions[memberKey] else { return .failedRead }

        switch position {
        case .onboarding:
            // Never in a ring, so no unload was possible: provably not-unloaded without a read.
            return .present(.voucher(.notUnloaded))
        case .suspended:
            // Once in a ring, none now: the alias key cannot be formed, so nothing can be said.
            return .present(.voucher(.unknown))
        case let .included(ringIndex):
            let aliasKey = CoinageChainState.aliasKey(index: index, exponent: exponent, ringIndex: ringIndex)
            if faults.unreadableAliases.contains(aliasKey) {
                // A failed alias read leaves the voucher present but its unload state unknown.
                return .present(.voucher(.unknown))
            }
            let unloaded = state.aliases[aliasKey] == true
            return .present(.voucher(unloaded ? .unloaded : .notUnloaded))
        }
    }
}
