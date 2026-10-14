package io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

@Serializable
enum class LegacyMultimediaType { IMAGE, VIDEO }

@Serializable
enum class LegacyAspectRatio { SQUARE, WIDE }

/**
 * RichTextContentLocal as it existed in V1/V2 — used `media` field with [LegacyMediaLocal].
 * Replaced by current RichTextContentLocal which uses `attachments`.
 */
@Serializable
class LegacyRichTextContentLocal(
    val text: String?,
    val media: List<LegacyMediaLocal>?
)

@Serializable
sealed interface LegacyMediaLocal {
    @Serializable
    @EnumIndex(0)
    data class ImageRemoteUrl(val url: String) : LegacyMediaLocal
}

/**
 * CoinagePaymentStatusLocal as it existed up to V4 — a plain enum.
 * Replaced by the current sealed CoinagePaymentStatusLocal whose Detected variant carries a Balance.
 */
@Serializable
enum class LegacyCoinagePaymentStatusLocal {
    DETECTING, DETECTED, TRANSFERRED, FAILED_DETECTION, FAILED_TRANSFER
}
