package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models.toUi
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

/**
 * The avatar of a room. The avatar that the product set has priority over the icon of the room. The image
 * loader cannot read a `data:` icon of the first version, so the resolver decodes it.
 */
suspend fun ChatResourceResolver.roomAvatar(roomIcon: ChatAvatar, productAvatar: ChatResourceModel?): AvatarUiModel {
    val hostAvatar = if (roomIcon is ChatAvatar.Url) {
        resolveIcon(roomIcon.url)
            .logFailure("Failed to resolve the icon of a room")
            .fold(onSuccess = { it.toAvatar() }, onFailure = { roomIcon.toUi() })
    } else {
        roomIcon.toUi()
    }

    if (productAvatar == null) return hostAvatar

    return resolve(productAvatar)
        .logFailure("Failed to resolve the avatar of a product room")
        .fold(onSuccess = { it.toAvatar() }, onFailure = { hostAvatar })
}

/** Changes the images of a product room into values that the image loader draws (02-DESIGN §6.2). */
interface ChatResourceResolver {
    suspend fun resolve(resource: ChatResourceModel): Result<ResolvedChatImage>

    /** Resolves an icon of the first version, which is a `data:` address or an `https` address. */
    fun resolveIcon(icon: String): Result<ResolvedChatImage>
}

class RealChatResourceResolver @Inject constructor(
    private val ipfsContentLookup: IpfsContentLookup,
) : ChatResourceResolver {
    override suspend fun resolve(resource: ChatResourceModel): Result<ResolvedChatImage> = when (resource) {
        is ChatResourceModel.Inline -> Result.success(ResolvedChatImage.Inline(resource.bytes.value))

        // This is the lookup of `lookup_preimage` in the host API: the raw block of the key on the IPFS gateway.
        is ChatResourceModel.Preimage -> ipfsContentLookup.getDefaultRawLink(resource.key.value)
            .map { ResolvedChatImage.Remote(it) }

        is ChatResourceModel.Url -> resolveIcon(resource.url)

        // The core seals user files only, never an avatar, an icon or a badge.
        is ChatResourceModel.Sealed -> Result.failure(IllegalArgumentException("A sealed file is not an image of a room"))
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
