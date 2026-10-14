package io.paritytech.polkadotapp.feature_balances_impl.data.type.assets.model

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.AsDictEnum
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
internal class AssetsAssetDetails(
    val status: Status,
    val isSufficient: Boolean,
    val minBalance: Balance,
    val issuer: AccountId
) {
    @Serializable
    @AsDictEnum
    enum class Status {
        @SerialName("Live")
        LIVE,

        @SerialName("Frozen")
        FROZEN,

        @SerialName("Destroying")
        DESTROYING
    }
}

internal val AssetsAssetDetails.Status.transfersFrozen: Boolean
    get() = this != AssetsAssetDetails.Status.LIVE
