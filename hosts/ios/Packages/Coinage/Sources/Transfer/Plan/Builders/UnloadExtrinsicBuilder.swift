import DurableTransactions
import ExtrinsicService
import Foundation
import KeyDerivation
import SDKLogger
@preconcurrency import SubstrateOperation
import SubstrateSdk

/// Declares a payment's recycler unloads: the vouchers each one redeems and the coins it mints.
///
/// Every unload of one call is built together, so they share one pinned block and one person proof, and
/// each gets its own free unload token. The token and the recycler revision are resolved **per build**,
/// never reused from an earlier attempt: a token is a quota-limited resource and a revision moves.
struct UnloadExtrinsicBuilder: Sendable {
    /// One recycler group's unload: the vouchers it redeems and the coins it mints, in order.
    struct Unload: Sendable {
        let vouchers: [Voucher]
        let outputs: [Coin]
    }

    let instanceId: CoinageInstanceId
    let voucherKeyFactory: any VoucherKeyDeriving
    let recyclerLoader: RecyclerReadinessLoading
    /// The origin factory is a shared, stateless service, but its protocol cannot carry `Sendable`: the
    /// app's conformer inherits from a base class, which Swift forbids a `Sendable` class from doing. The
    /// reference is only read here, so it is vouched for at the property rather than for the whole type.
    nonisolated(unsafe) let originFactory: OriginCreating
    let blockInfoProvider: any BlockInfoProviding
    let quotaTracker: any UnloadQuotaTracking
    let factory: any DurableTxMaking
    let chainId: ChainId
    let logger: SDKLoggerProtocol?

    func build(_ unloads: [Unload], currentDate: Date) async throws -> [ExtrinsicBuiltModel] {
        guard !unloads.isEmpty else { return [] }

        // One pinned block for every read below, so the origins and the revisions describe the same state.
        let blockHash = try await blockInfoProvider.fetchCurrentHash()

        let origins = try await originFactory.createAsUnloadTokenOrigins(
            voucherGroups: unloads.map(\.vouchers),
            currentDate: currentDate,
            blockHash: blockHash
        )

        guard origins.count == unloads.count else {
            throw TransferStrategyError.missingRecyclerInfo
        }

        let keys = try unloads.map { try Self.recyclerKey(of: $0) }

        // Several calls can share a recycler when it holds more vouchers than one call may unload,
        // so the query is deduplicated and each unload looks its own revision up by key.
        var seenKeys = Set<RecyclerKey>()
        let queriedKeys = keys.filter { seenKeys.insert($0).inserted }
        let revisions = try await recyclerLoader.fetchRevisions(for: queriedKeys, blockHash: blockHash)

        let parts = try zip(zip(unloads, origins), keys).map { pair, key in
            guard let revision = revisions[key] else {
                throw TransferStrategyError.missingRecyclerInfo
            }

            let call = try call(for: pair.0, key: key, revision: revision)

            return DurableTxRequest(
                builder: { try $0.adding(call: call.callAsFunction()) },
                origin: pair.1
            )
        }

        let models = try await factory.makeExtrinsics(parts, chainId: chainId)

        // Only a build that actually produced extrinsics has spent tokens.
        await quotaTracker.noteUnloadHappened(count: models.count)
        logger?.debug("Built \(models.count) unload extrinsic(s)")

        return models
    }
}

private extension UnloadExtrinsicBuilder {
    /// Every voucher of one unload sits in the same recycler, so any of them names it.
    static func recyclerKey(of unload: Unload) throws -> RecyclerKey {
        guard let voucher = unload.vouchers.first, let recycler = voucher.recycler else {
            throw TransferStrategyError.missingRecyclerInfo
        }

        return RecyclerKey(exponent: voucher.exponent, index: recycler.index)
    }

    func call(
        for unload: Unload,
        key: RecyclerKey,
        revision: UInt32
    ) throws -> CoinagePallet.Calls.UnloadRecyclerIntoCoins {
        let aliases = try unload.vouchers.map {
            try voucherKeyFactory.createKeyManager(for: $0)
                .deriveAlias(for: UnloadTokenContextBuilder.recyclerAliasContext)
        }

        var grouped: [Int16: [Data]] = [:]

        for coin in unload.outputs {
            grouped[coin.exponent, default: []].append(coin.publicKey)
        }

        let destinations = grouped.map {
            CoinagePallet.Calls.Split.SplitDestination(exponent: $0.key, accounts: $0.value)
        }

        return CoinagePallet.Calls.UnloadRecyclerIntoCoins(
            instanceId: instanceId,
            aliases: aliases,
            value: Int8(key.exponent),
            index: key.index,
            revision: revision,
            splitInto: destinations.sorted { $0.exponent < $1.exponent }
        )
    }
}
