package io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import kotlinx.serialization.Serializable

// These types keep the field order of the Chat Extension v2 wire types (`v01::chat`), so a value that
// has a wire twin encodes to the same bytes. Any change to them requires a content migration.

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
}

@Serializable
class ChatActionButtonLocal(val actionId: String, val title: String)

@Serializable
enum class ChatActionsLayoutLocal { COLUMN, GRID }
