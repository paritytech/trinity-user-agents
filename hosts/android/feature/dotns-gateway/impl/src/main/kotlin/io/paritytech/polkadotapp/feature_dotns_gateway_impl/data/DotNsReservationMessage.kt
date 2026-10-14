package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.EncodedPublicKey
import kotlinx.serialization.Serializable

@Serializable
class DotNsReservationMessage(
    val prefix: ByteArraySerializable,
    @FixedLength(32) val candidate: ByteArraySerializable,
    @FixedLength(32) val attester: ByteArraySerializable,
    val usernameBase: ByteArraySerializable,
    val chatKey: ByteArraySerializable,
    val reservedBaseLabel: ByteArray?,
    val signedAt: Long
) {
    companion object {
        private const val RESERVE_MESSAGE_PREFIX = "pop:dotns-gateway:reserve"

        fun signingPayload(
            candidate: AccountId,
            attester: AccountId,
            usernameBase: String,
            chatKey: EncodedPublicKey,
            reservedBaseLabel: String?,
            signedAt: Long
        ): ByteArray {
            return DotNsReservationMessage(
                prefix = RESERVE_MESSAGE_PREFIX.encodeToByteArray(),
                candidate = candidate.value,
                attester = attester.value,
                usernameBase = usernameBase.encodeToByteArray(),
                chatKey = chatKey.value,
                reservedBaseLabel = reservedBaseLabel?.encodeToByteArray(),
                signedAt = signedAt
            ).scaleEncodeBinary()
        }
    }
}
