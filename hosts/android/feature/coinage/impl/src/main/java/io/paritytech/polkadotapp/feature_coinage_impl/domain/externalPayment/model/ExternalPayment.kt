package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment.ExternalPaymentKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageKeyIndex

data class ExternalPayment(
    val key: ExternalPaymentKey,
    val amount: Balance,
    val destination: AccountId,
    val stage: Stage,
    val createdAt: Long,
    val updatedAt: Long,
) {
    sealed interface Stage {
        data object EnsureVouchers : Stage

        data class AwaitRecycling(val exactVoucherKeys: List<CoinageKeyIndex>) : Stage

        data class OffboardVouchers(
            val selectedVoucherKeys: List<CoinageKeyIndex>,
            val surplus: Balance,
        ) : Stage

        data object Completed : Stage

        data class PartiallyCompleted(val claimed: Balance) : Stage

        data class Failed(val reason: String) : Stage
    }

    companion object {
        fun new(
            key: ExternalPaymentKey,
            amount: Balance,
            destination: AccountId,
        ): ExternalPayment {
            val now = System.currentTimeMillis()
            return ExternalPayment(
                key = key,
                amount = amount,
                destination = destination,
                stage = Stage.EnsureVouchers,
                createdAt = now,
                updatedAt = now,
            )
        }
    }
}
