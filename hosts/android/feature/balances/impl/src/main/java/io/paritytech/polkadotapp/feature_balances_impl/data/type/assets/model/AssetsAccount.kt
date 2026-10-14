package io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsDictEnum
import io.paritytech.polkadotapp.chains.network.binding.Balance
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
internal class AssetsAccount(
    val balance: Balance,
    val status: AccountStatus
) {
    @Serializable
    @AsDictEnum
    enum class AccountStatus {
        @SerialName("Liquid")
        LIQUID,

        @SerialName("Frozen")
        FROZEN,

        @SerialName("Blocked")
        BLOCKED
    }

    companion object {
        fun empty() = AssetsAccount(
            balance = Balance.ZERO,
            status = AccountStatus.LIQUID
        )
    }
}

internal fun AssetsAccount?.orEmpty(): AssetsAccount {
    return this ?: AssetsAccount.empty()
}

internal val AssetsAccount.isBalanceFrozen: Boolean
    get() = status == AssetsAccount.AccountStatus.BLOCKED || status == AssetsAccount.AccountStatus.FROZEN

internal val AssetsAccount.canAcceptFunds: Boolean
    get() = status != AssetsAccount.AccountStatus.BLOCKED
