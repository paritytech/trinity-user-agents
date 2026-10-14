package io.paritytech.polkadotapp.feature_device_sync_impl.data.model.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.common.domain.model.scale.X25519PublicKeyScale
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalMessageScale
import kotlinx.serialization.Serializable

@Serializable
class SyncUpdateScale(
    val id: UInt,
    val entities: List<SyncEntityScale>,
    val timePoint: ULong,
)

@Serializable
class SyncUpdateAckScale(
    val id: UInt,
)

/** Outer envelope on the data channel — Update or Ack. */
@Serializable
sealed interface SyncMessageScale {
    @Serializable
    @EnumIndex(0)
    class Update(val update: SyncUpdateScale) : SyncMessageScale

    @Serializable
    @EnumIndex(1)
    class Ack(val ack: SyncUpdateAckScale) : SyncMessageScale
}

@Serializable
sealed interface SyncEntityScale {
    @Serializable
    @EnumIndex(0)
    class Devices(val devices: List<LocalDeviceScale>) : SyncEntityScale

    @Serializable
    @EnumIndex(1)
    class ChatsAdded(val chats: List<ChatIdScale>) : SyncEntityScale

    @Serializable
    @EnumIndex(2)
    class ChatsRemoved(val chats: List<ChatIdScale>) : SyncEntityScale

    @Serializable
    @EnumIndex(3)
    class Messages(val messages: List<LocalMessageScale>) : SyncEntityScale
}

@Serializable
class LocalDeviceScale(
    @FixedLength(32)
    val statementAccountId: ByteArray,
    val encryptionPublicKey: X25519PublicKeyScale,
    val status: DeviceStatusScale,
    val lastUpdate: ULong,
)

@Serializable
enum class DeviceStatusScale {
    @EnumIndex(0)
    ACTIVE,
}

@Serializable
sealed interface ChatIdScale {
    @Serializable
    @EnumIndex(0)
    class Contact(
        @FixedLength(32)
        val accountId: ByteArray,
    ) : ChatIdScale
}
