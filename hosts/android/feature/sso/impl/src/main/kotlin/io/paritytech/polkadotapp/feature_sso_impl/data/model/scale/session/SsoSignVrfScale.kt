package io.paritytech.polkadotapp.feature_sso_impl.data.model.scale.session

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.SignVrfError
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfSignature
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import kotlinx.serialization.Serializable

// RFC-0023 `VrfTranscriptItem`: one `append_message(label, value)` call.
@Serializable
data class VrfTranscriptItemScale(
    val label: DataByteArray,
    val value: DataByteArray,
)

// RFC-0023 `VrfSignature`: schnorrkel VRFPreOut (32) + VRFProof (64).
@Serializable
data class VrfSignatureScale(
    val preOutput: DataByteArray,
    val proof: DataByteArray,
)

// RFC-0023 `HostAccountSignVrfError`. Variant order is the cross-host wire contract; NotConnected
// exists to keep the index space aligned with other hosts — this host has no session gate to fail.
@Serializable
sealed class SsoSignVrfErrorScale {
    @Serializable
    @EnumIndex(0)
    data object NotConnected : SsoSignVrfErrorScale()

    @Serializable
    @EnumIndex(1)
    data object Rejected : SsoSignVrfErrorScale()

    @Serializable
    @EnumIndex(2)
    data class Unknown(val reason: String) : SsoSignVrfErrorScale()
}

fun VrfTranscriptItem.toScale(): VrfTranscriptItemScale = VrfTranscriptItemScale(label, value)

fun VrfTranscriptItemScale.toDomain(): VrfTranscriptItem = VrfTranscriptItem(label, value)

fun VrfSignature.toScale(): VrfSignatureScale = VrfSignatureScale(preOutput, proof)

fun SignVrfError.toSsoScale(): SsoSignVrfErrorScale = when (this) {
    is SignVrfError.Rejected -> SsoSignVrfErrorScale.Rejected
    is SignVrfError.TranscriptTooLarge -> SsoSignVrfErrorScale.Unknown(message.orEmpty())
    is SignVrfError.Unknown -> SsoSignVrfErrorScale.Unknown(message.orEmpty())
}
