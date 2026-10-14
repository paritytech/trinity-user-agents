package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResource
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import io.paritytech.polkadotapp.tools_ipfs_api.getDefaultRawLink
import java.util.Base64
import javax.inject.Inject

sealed interface ResolvedChatImage {
    val model: Any

    class Inline(val bytes: ByteArray) : ResolvedChatImage {
        override val model: Any get() = bytes

        override fun equals(other: Any?): Boolean = other is Inline && bytes.contentEquals(other.bytes)

        override fun hashCode(): Int = bytes.contentHashCode()
    }

    /** The image loader gets the bytes from [url] and keeps them in its disk cache. */
    data class Remote(val url: String) : ResolvedChatImage {
        override val model: Any get() = url
    }
}

fun ResolvedChatImage.toAvatar(): AvatarUiModel = when (this) {
    is ResolvedChatImage.Inline -> AvatarUiModel.Inline(bytes)
    is ResolvedChatImage.Remote -> AvatarUiModel.Image(url)
}

/** Changes the images of a product room into values that the image loader draws (02-DESIGN §6.2). */
interface ChatResourceResolver {
    suspend fun resolve(resource: ChatResource): Result<ResolvedChatImage>

    /** Resolves an icon of the first version, which is a `data:` address or an `https` address. */
    fun resolveIcon(icon: String): Result<ResolvedChatImage>
}

class RealChatResourceResolver @Inject constructor(
    private val ipfsContentLookup: IpfsContentLookup,
) : ChatResourceResolver {
    override suspend fun resolve(resource: ChatResource): Result<ResolvedChatImage> = when (resource) {
        is ChatResource.Inline -> Result.success(ResolvedChatImage.Inline(resource.bytes.value))

        // This is the lookup of `lookup_preimage` in the host API: the raw block of the key on the IPFS gateway.
        is ChatResource.Preimage -> ipfsContentLookup.getDefaultRawLink(resource.key.value)
            .map { ResolvedChatImage.Remote(it) }
    }

    override fun resolveIcon(icon: String): Result<ResolvedChatImage> {
        if (!icon.startsWith(DATA_SCHEME)) return Result.success(ResolvedChatImage.Remote(icon))

        return runCatching { ResolvedChatImage.Inline(decodeDataUrl(icon)) }
    }

    private fun decodeDataUrl(url: String): ByteArray {
        val separator = url.indexOf(',')
        require(separator > 0) { "A data address has no payload" }

        val header = url.substring(DATA_SCHEME.length, separator)
        require(header.endsWith(BASE64_SUFFIX)) { "Only base64 data addresses are images" }

        return Base64.getDecoder().decode(url.substring(separator + 1))
    }

    private companion object {
        const val DATA_SCHEME = "data:"
        const val BASE64_SUFFIX = ";base64"
    }
}
