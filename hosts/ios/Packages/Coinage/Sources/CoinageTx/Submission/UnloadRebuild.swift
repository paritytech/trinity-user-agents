import AsyncExtensions
import DurableTransactions
import ExtrinsicService
import Foundation
import FoundationExt

/// A payment's recycler unload, read back from the ledger: the vouchers it redeems and the coins it
/// mints, in order.
///
/// A voucher counts as present while it sits in a recycler — that is where an unload proves it, and one
/// that left was redeemed by something else. The vouchers are re-read after the look, so every one
/// carries the recycler location it is proven in *now* rather than the one it had when the payment was
/// planned.
struct UnloadRebuild: CoinageRebuild {
    struct Unload: Sendable {
        let voucherIndices: [CoinageKeyIndex]
        let outputs: [Coin]
    }

    let coinService: any CoinServiceProtocol
    let voucherService: any VoucherServiceProtocol
    let builder: UnloadExtrinsicBuilder
    /// The local tracked-voucher snapshots the gate watches — the same rows ``build(_:)`` reads.
    let voucherSnapshots: @Sendable () -> AnyAsyncSequence<[TrackedVoucher]>
    let dateProvider: any DateProviding

    func terms(of params: Data) -> RebuildTerms? {
        guard let transfer = try? CoinageSubmissionParams.decodeTransfer(params) else { return nil }

        return RebuildTerms(deadline: transfer.buildUntil, retriesFailures: transfer.retryFailures)
    }

    func resolve(
        _ transactions: [ScheduledDurableTx],
        assets: [CoinageTxId: CoinageTxEntry]
    ) async throws -> [CoinageTxId: Unload] {
        let outputKeys = assets.values.reduce(into: Set<PublicKey>()) {
            $0.formUnion($1.outputs.map(\.publicKey))
        }
        let inputKeys = assets.values.reduce(into: Set<PublicKey>()) {
            $0.formUnion($1.inputs.map(\.publicKey))
        }

        let coins = try await coinService.fetchCoins(publicKeys: outputKeys)
        let vouchers = try await voucherService.fetchVouchers(publicKeys: inputKeys)

        let coinsByKey = coins.reduce(into: [PublicKey: Coin]()) { $0[$1.publicKey] = $1 }
        let heldVouchers = Set(vouchers.map(\.publicKey))

        return transactions.reduce(into: [:]) { resolved, transaction in
            guard let entry = assets[transaction.id],
                  let unload = unload(of: entry, coins: coinsByKey, heldVouchers: heldVouchers)
            else {
                return
            }

            resolved[transaction.id] = unload
        }
    }

    func inputs(of transaction: Unload) -> Set<CoinageKeyIndex> {
        Set(transaction.voucherIndices)
    }

    func presence(of _: Set<CoinageKeyIndex>) async throws -> AnyAsyncSequence<Set<CoinageKeyIndex>> {
        voucherRecyclerPresence(snapshots: voucherSnapshots())
    }

    func build(_ transactions: [Unload]) async throws -> [ExtrinsicBuiltModel] {
        // Re-read after the look, so every voucher carries the recycler location it is proven in now.
        let indices = transactions.flatMap(\.voucherIndices)
        let vouchers = try await voucherService.fetchTracked(derivationIndices: Set(indices))
            .reduce(into: [CoinageKeyIndex: Voucher]()) { $0[$1.voucher.derivationIndex] = $1.voucher }

        let unloads = try transactions.map { transaction in
            try UnloadExtrinsicBuilder.Unload(
                vouchers: transaction.voucherIndices.map {
                    guard let voucher = vouchers[$0] else { throw TransferStrategyError.missingRecyclerInfo }

                    return voucher
                },
                outputs: transaction.outputs
            )
        }

        return try await builder.build(unloads, currentDate: dateProvider.read())
    }
}

private extension UnloadRebuild {
    /// Every input a voucher we still hold and every output a coin we recorded, or nothing: a partial
    /// unload would redeem or mint something other than what was registered.
    func unload(
        of entry: CoinageTxEntry,
        coins: [PublicKey: Coin],
        heldVouchers: Set<PublicKey>
    ) -> Unload? {
        guard !entry.inputs.isEmpty, !entry.outputs.isEmpty else { return nil }

        var voucherIndices: [CoinageKeyIndex] = []

        for input in entry.inputs {
            guard case let .recyclerVoucher(index, key) = input, heldVouchers.contains(key) else {
                return nil
            }

            voucherIndices.append(index)
        }

        var outputs: [Coin] = []

        for output in entry.outputs {
            guard output.isCoin, let coin = coins[output.publicKey] else { return nil }

            outputs.append(coin)
        }

        return Unload(voucherIndices: voucherIndices, outputs: outputs)
    }
}
