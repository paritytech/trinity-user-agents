package io.paritytech.polkadotapp.feature_coinage_api.domain.externalPayment

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.RecyclerVoucher

sealed interface ExternalPaymentPlan {
    class Ready(val offboarding: VoucherOffboarding) : ExternalPaymentPlan {
        override fun toString(): String {
            return "Ready(vouchers=${offboarding.vouchers.size}, surplus=${offboarding.surplus})"
        }
    }

    /** [exactVouchers] are offboarded as they are, next to what [coinsToLoad] turn into once recycled. */
    class LoadCoins(
        val coinsToLoad: List<Coin>,
        val exactVouchers: List<RecyclerVoucher>,
    ) : ExternalPaymentPlan {
        override fun toString(): String {
            return "LoadCoins(coins=${coinsToLoad.size}, exactVouchers=${exactVouchers.size})"
        }
    }

    data class NotEnoughAmount(
        val activeVouchers: Balance,
        val activeCoins: Balance,
        val deficitToCoverWithCoins: Balance,
    ) : ExternalPaymentPlan
}

class VoucherOffboarding(
    val vouchers: List<RecyclerVoucher>,
    val surplus: Balance,
)
