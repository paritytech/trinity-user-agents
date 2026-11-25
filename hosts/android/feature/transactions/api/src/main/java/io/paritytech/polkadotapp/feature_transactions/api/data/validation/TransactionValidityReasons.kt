package io.paritytech.polkadotapp.feature_transactions.api.data.validation

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.SerializedFallback
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import kotlinx.serialization.Serializable

@Serializable
@SerializedFallback("Fallback")
sealed class InvalidTransaction {
    @Serializable
    data object Call : InvalidTransaction()

    @Serializable
    data object Payment : InvalidTransaction()

    @Serializable
    data object Future : InvalidTransaction()

    @Serializable
    data object Stale : InvalidTransaction()

    @Serializable
    data object BadProof : InvalidTransaction()

    @Serializable
    data object AncientBirthBlock : InvalidTransaction()

    @Serializable
    data object ExhaustsResources : InvalidTransaction()

    @Serializable
    @TransientStruct
    data class Custom(val code: UByte) : InvalidTransaction()

    @Serializable
    data object BadMandatory : InvalidTransaction()

    @Serializable
    data object MandatoryValidation : InvalidTransaction()

    @Serializable
    data object BadSigner : InvalidTransaction()

    // Absorbs any future runtime variant this client doesn't know about, so decoding never crashes.
    @Serializable
    data object Fallback : InvalidTransaction()
}

@Serializable
@SerializedFallback("Fallback")
sealed class UnknownTransaction {
    @Serializable
    data object CannotLookup : UnknownTransaction()

    @Serializable
    data object NoUnsignedValidationFunction : UnknownTransaction()

    @Serializable
    @TransientStruct
    data class Custom(val code: UByte) : UnknownTransaction()

    @Serializable
    data object Fallback : UnknownTransaction()
}
