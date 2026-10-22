package io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

// These types keep the field order of the Chat Extension v2 wire types (`v01::chat`), so a value that
// has a wire twin encodes to the same bytes. Any change to them requires a content migration. The kinds that
// Chat Extension v2 no longer has (`Actions`, and the `Inline`, `Preimage` and `Sealed` resources) stay so that
// stored messages still read; nothing writes them.

@Serializable
sealed interface ProductMessageBodyLocal {
    @Serializable
    @EnumIndex(0)
    class RichText(val text: String?, val media: List<ChatMediaItemLocal>) : ProductMessageBodyLocal

    @Serializable
    @EnumIndex(1)
    class Actions(
        val text: String?,
        val actions: List<ChatActionButtonLocal>,
        val layout: ChatActionsLayoutLocal,
    ) : ProductMessageBodyLocal
}

@Serializable
sealed interface ChatMediaItemLocal {
    @Serializable
    @EnumIndex(0)
    class Image(val resource: ChatResourceLocal, val meta: ChatMediaMetaLocal) : ChatMediaItemLocal

    @Serializable
    @EnumIndex(1)
    class Video(val resource: ChatResourceLocal, val meta: ChatMediaMetaLocal) : ChatMediaItemLocal

    @Serializable
    @EnumIndex(2)
    class File(val resource: ChatResourceLocal, val meta: ChatMediaMetaLocal, val fileName: String) : ChatMediaItemLocal
}

/** The wire meta ends with `duration_ms`, which a stored body does not keep: a video of a kept message has no length. */
@Serializable
class ChatMediaMetaLocal(
    val mimeType: String,
    val sizeBytes: Long?,
    val width: Int?,
    val height: Int?,
    val placeholder: String?,
    val alt: String?,
)

/** Index 2 has no wire twin: it keeps the address of a v1 message body. */
@Serializable
sealed interface ChatResourceLocal {
    @Serializable
    @EnumIndex(0)
    class Inline(val mimeType: String, val bytes: ByteArray) : ChatResourceLocal

    @Serializable
    @EnumIndex(1)
    class Preimage(val key: ByteArray) : ChatResourceLocal

    @Serializable
    @EnumIndex(2)
    class Url(val url: String) : ChatResourceLocal

    /** [size] holds the bits of a `u64`. */
    @Serializable
    @EnumIndex(3)
    class Sealed(
        val parts: List<ByteArray>,
        val key: ByteArray,
        val nonce: ByteArray,
        val size: Long,
        val digest: ByteArray,
    ) : ChatResourceLocal

    /** [size] holds the bits of a `u64`. */
    @Serializable
    @EnumIndex(4)
    class Hop(val identifier: ByteArray, val ticket: ByteArray, val node: String, val size: Long) : ChatResourceLocal
}

@Serializable
class ChatActionButtonLocal(val actionId: String, val title: String)

@Serializable
enum class ChatActionsLayoutLocal { COLUMN, GRID }
