package io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher

interface ExternalPaymentPlanner {
    /**
     * Prefers funds that cost no privacy to spend, and falls back to anything the chain would accept. Callers
     * that may reach the fallback are expected to have confirmed the privacy loss with the user beforehand, see
     * [canPayPrivately].
     */
    suspend fun plan(amount: Balance): Result<ExternalPaymentPlan>

    /**
     * Whether [plan] would pay [amount] from private vouchers alone. Anything else — a voucher still gaining
     * privacy, or a coin loaded just to be unloaded — gives up privacy the user should be asked about first.
     */
    suspend fun canPayPrivately(amount: Balance): Result<Boolean>

    /**
     * Picks a subset of [availableVouchers] that reaches [target] amount. Used after loading extra coins
     * into vouchers, when the plan was [ExternalPaymentPlan.LoadCoins].
     * @return failure in case total balance of [availableVouchers] is not sufficient to cover [target]
     */
    suspend fun pickOffboarding(
        availableVouchers: List<RecyclerVoucher>,
        target: Balance,
    ): Result<VoucherOffboarding>
}
