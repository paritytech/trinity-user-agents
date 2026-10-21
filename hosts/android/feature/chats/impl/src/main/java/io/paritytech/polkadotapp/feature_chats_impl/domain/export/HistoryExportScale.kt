package io.paritytech.polkadotapp.feature_chats_impl.domain.export

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.feature_chats_transport_protocol.scale.LocalStatusScale
import kotlinx.serialization.Serializable

// The chat product's `HistoryExportCodec` (`packages/core/src/wire/history.ts`).
@Serializable
sealed interface HistoryExportScale {
    @Serializable
    @EnumIndex(0)
    class V1(
        val contacts: List<HistoryContactScale>,
        val requests: List<HistoryRequestScale>,
        val messages: List<HistoryMessageScale>,
    ) : HistoryExportScale
}

@Serializable
class HistoryContactScale(
    @FixedLength(32) val accountId: ByteArray,
    val username: String?,
    @FixedLength(32) val chatKey: ByteArray,
    val pushToken: ByteArray?,
    val pushPlatform: UByte?,
    val voipToken: ByteArray?,
    val blocked: Boolean,
    val peerLeft: Boolean,
    val addedAt: ULong,
    val establishedAt: ULong?,
    val devices: List<HistoryDeviceScale>,
)

@Serializable
class HistoryDeviceScale(
    @FixedLength(32) val statementAccountId: ByteArray,
    @FixedLength(32) val encryptionPublicKey: ByteArray,
)

@Serializable
class HistoryRequestScale(
    val id: String,
    @FixedLength(32) val peer: ByteArray,
    val timestamp: ULong,
    val outgoing: Boolean,
    val status: UByte,
    val delivered: Boolean,
    val welcome: String?,
)

@Serializable
class HistoryMessageScale(
    @FixedLength(32) val peerId: ByteArray,
    val message: ByteArray,
    val status: LocalStatusScale,
    val order: ULong,
)
