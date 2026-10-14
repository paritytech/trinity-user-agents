import DurableTransactions
import Foundation
import KeyDerivation

/// What a voucher's recycler alias says at one block — three-valued, because a voucher can be present
/// yet leave its unload state unreadable: Suspended from its ring (the alias key needs a ring index it
/// no longer has), or a failed alias read.
public enum VoucherAliasPresence: Sendable, Equatable {
    case unloaded
    case notUnloaded
    case unknown
}

/// On-chain presence of one asset at one block.
///
/// A coin is simply there or not. A recycler voucher also carries what its alias says: a successful
/// unload marks the alias without removing the recycler mapping, so a spent voucher can still read
/// present, and a present voucher may still leave that `.unknown`.
public enum AssetPresence: Sendable, Equatable {
    case coin
    case voucher(VoucherAliasPresence)
}

/// Coinage's reads of the chain at a pinned block: the presence of its coins and vouchers.
///
/// Three-valued: every read returns `failedRead` rather than throwing on transport failure, an unknown
/// block, a key missing from a batched response, or an undecodable value, so a read failure can never be
/// mistaken for absence.
public protocol CoinageStateReading: Sendable {
    /// Presence of each input at `block`, in the order given.
    func readInputs(_ inputs: [CoinageTxInput], at block: BlockRef) async -> [ReadResult<AssetPresence>]

    /// Presence of each output at `block`, in the order given.
    func readOutputs(_ outputs: [OwnAsset], at block: BlockRef) async -> [ReadResult<AssetPresence>]
}

/// Concrete ``CoinageStateReading`` over the coin and voucher storage queries.
final class CoinageStateReader: CoinageStateReading {
    private let coinQuery: any CoinOnChainQuerying
    private let voucherQuery: any VoucherOnChainQuerying

    init(coinQuery: any CoinOnChainQuerying, voucherQuery: any VoucherOnChainQuerying) {
        self.coinQuery = coinQuery
        self.voucherQuery = voucherQuery
    }

    func readInputs(_ inputs: [CoinageTxInput], at block: BlockRef) async -> [ReadResult<AssetPresence>] {
        await read(assets: inputs, at: block)
    }

    func readOutputs(_ outputs: [OwnAsset], at block: BlockRef) async -> [ReadResult<AssetPresence>] {
        await read(assets: outputs.map(\.asInput), at: block)
    }
}

private extension CoinageStateReader {
    /// A coin whose key cannot be derived is left out of both batches, so its position keeps the
    /// `failedRead` it starts with.
    func read(assets: [CoinageTxInput], at block: BlockRef) async -> [ReadResult<AssetPresence>] {
        guard !assets.isEmpty else { return [] }

        let coins = assets.enumerated().compactMap { position, asset -> (position: Int, key: Data)? in
            asset.isCoin ? (position, asset.publicKey) : nil
        }

        let vouchers = assets.enumerated().compactMap { position, asset -> (position: Int, index: CoinageKeyIndex)? in
            guard case let .recyclerVoucher(index, _) = asset else { return nil }
            return (position, index)
        }

        async let coinReads = fetchCoins(keys: coins.map(\.key), at: block)
        async let voucherReads = fetchVouchers(indices: vouchers.map(\.index), at: block)

        let (coinResults, voucherResults) = await (coinReads, voucherReads)

        var results = [ReadResult<AssetPresence>](repeating: .failedRead, count: assets.count)
        for (asset, result) in zip(coins, coinResults) {
            results[asset.position] = result
        }
        for (asset, result) in zip(vouchers, voucherResults) {
            results[asset.position] = result
        }

        return results
    }

    /// Batched coin read. A response shorter than the request means a key is missing from it, which
    /// must be treated as no verdict rather than absence.
    func fetchCoins(keys: [Data], at block: BlockRef) async -> [ReadResult<AssetPresence>] {
        guard !keys.isEmpty else { return [] }

        guard let responses = try? await coinQuery.fetchCoins(for: keys, atBlockHash: block.hash),
              responses.count == keys.count
        else {
            return Array(repeating: .failedRead, count: keys.count)
        }

        return responses.map { $0 == nil ? .absent : .present(.coin) }
    }

    /// A voucher is present whenever it is a recycler member — Onboarding, Suspended or Included. A nil
    /// response means it is in no recycler, which is not proof of consumption: archival removes the
    /// membership while the voucher is still redeemable, and a voucher's disappearance from the recycler
    /// must never read as absence (only a coin's absence is consumption). So an absent membership reads
    /// `failedRead` — an unknown the rules withhold a verdict on — rather than `absent`. The alias, not
    /// membership, is a voucher's only proof of being spent.
    func fetchVouchers(indices: [CoinageKeyIndex], at block: BlockRef) async -> [ReadResult<AssetPresence>] {
        guard !indices.isEmpty else { return [] }

        guard let responses = try? await voucherQuery.fetchVouchers(for: indices, atBlockHash: block.hash),
              responses.count == indices.count
        else {
            return Array(repeating: .failedRead, count: indices.count)
        }

        return responses.map { info in
            guard let info else { return .failedRead }
            return .present(.voucher(info.aliasPresence))
        }
    }
}
