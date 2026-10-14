package io.paritytech.polkadotapp.feature_chats_transport_protocol.scale

import androidx.annotation.Keep
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.scale.X25519PublicKeyScale
import kotlinx.serialization.Serializable

@Serializable
@Keep
class DeviceInfoScale(
    @FixedLength(32)
    val statementAccountId: ByteArray,
    val encryptionPublicKey: X25519PublicKeyScale,
)
