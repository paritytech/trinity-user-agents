import Foundation
import SDKLogger
import SubstrateSdk

/// The last word on a durability group for a caller that holds only the group — a top-up whose source
/// secret is gone cannot re-run its claim, but what earlier runs registered still settles and is worth
/// exactly what its finalized outputs are.
public protocol CoinageGroupVerdictResolving: Sendable {
    /// Awaits every live entry under `groupId` to settle, then values what finalized against `amount`.
    /// Throws when the group cannot be observed: an unknown state must never become a verdict.
    func settledVerdict(
        groupId: CoinageTxGroupId,
        amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> CoinageTransferDetection
}

final class CoinageGroupVerdictResolver: CoinageGroupVerdictResolving, @unchecked Sendable {
    private let txService: any CoinageTxServicing
    private let coinService: any CoinServiceProtocol
    private let voucherService: any VoucherServiceProtocol

    init(
        txService: any CoinageTxServicing,
        coinService: any CoinServiceProtocol,
        voucherService: any VoucherServiceProtocol
    ) {
        self.txService = txService
        self.coinService = coinService
        self.voucherService = voucherService
    }

    func settledVerdict(
        groupId: CoinageTxGroupId,
        amount: Balance,
        context: DenominationBreakdownContext
    ) async throws -> CoinageTransferDetection {
        var last: [CoinageTxEntry] = []
        for try await states in txService.subscribeOperationGroupStatuses(groupId) {
            last = states
            if states.allSatisfy({ !$0.status.isLive }) { break }
        }

        let value = try await value(of: last.finalizedSuccess(), context: context)
        return .verdict(finalized: value, of: amount)
    }
}

private extension CoinageGroupVerdictResolver {
    /// The planks `entries` minted — coins and vouchers looked up by their output keys, so a group
    /// registered by either claim service values the same way.
    func value(of entries: [CoinageTxEntry], context: DenominationBreakdownContext) async throws -> Balance {
        let outputs = entries.flatMap(\.outputs)
        let coinKeys = Set(outputs.filter(\.isCoin).map(\.publicKey))
        let voucherKeys = Set(outputs.filter { !$0.isCoin }.map(\.publicKey))

        let coins = coinKeys.isEmpty ? [] : try await coinService.fetchCoins(publicKeys: coinKeys)
        let vouchers = voucherKeys.isEmpty ? [] : try await voucherService.fetchVouchers(publicKeys: voucherKeys)

        let coinValue = coins.reduce(Balance(0)) { $0 + context.valueInPlanks(for: $1.exponent) }
        return vouchers.reduce(coinValue) { $0 + context.valueInPlanks(for: $1.exponent) }
    }
}
