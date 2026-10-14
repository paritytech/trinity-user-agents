package io.paritytech.polkadotapp.feature_coinage_impl.data.model

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import kotlinx.serialization.Serializable

@Keep
@Serializable
sealed interface OnChainAliasState {
    @Keep
    @Serializable
    data class Locked(
        val reason: OnChainLockReason,
        val until: BigIntegerSerializable,
    ) : OnChainAliasState

    @Keep
    @Serializable
    data object Unloaded : OnChainAliasState
}

@Keep
@Serializable
sealed interface OnChainLockReason {
    @Keep
    @Serializable
    data class FailedDispatch(val retries: Int) : OnChainLockReason
}
