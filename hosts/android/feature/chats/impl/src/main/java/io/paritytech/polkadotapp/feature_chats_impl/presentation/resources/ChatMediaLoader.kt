package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFiles
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ChatOutboxPreviews
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileCache
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException
import java.security.MessageDigest
import java.util.Base64
import javax.inject.Inject

/**
 * Gives the media items of product messages as local files (02-DESIGN §6.2). The image loader draws the file, the
 * player plays it, and other apps open it. All the media of chat rooms share one bounded cache (04-TESTING B6).
 */
interface ChatMediaLoader {
    /**
     * The file that holds the bytes of [resource]. [extension] is the file extension for the MIME type of the item, so
     * that the image loader and other apps know the type of the file. A failure means that the bytes are not
     * available now, for example because the HOP node did not answer. A later call tries again. [progress] gets the
     * bytes received and the total size while a HOP file downloads.
     */
    suspend fun load(
        resource: ChatResourceModel,
        extension: String?,
        progress: ((received: Long, total: Long) -> Unit)? = null,
    ): Result<File>
}

class RealChatMediaLoader @Inject constructor(
    private val cache: ChatResourceDiskCache,
    private val dispatchers: CoroutineDispatchers,
    private val hopFiles: ChatHopFiles,
    private val hopCache: ChatHopFileCache,
    private val outboxPreviews: ChatOutboxPreviews = ChatOutboxPreviews { null },
) : ChatMediaLoader {
    override suspend fun load(
        resource: ChatResourceModel,
        extension: String?,
        progress: ((received: Long, total: Long) -> Unit)?,
    ): Result<File> = withContext(dispatchers.io) {
        when (resource) {
            // The node drops an acked file, so it stays in the store of opened files; the ack follows the store.
            is ChatResourceModel.Hop -> {
                hopCache.file(resource, extension)?.let { return@withContext Result.success(it) }

                hopFiles.open(resource, progress).flatMap { opened ->
                    hopCache.store(resource, extension, opened.bytes).onSuccess { hopFiles.ack(resource, opened.entries) }
                }
            }

            // An attachment that waits in the outbox shows its kept file (07-LATER §7).
            is ChatResourceModel.Url -> outboxPreviews.file(resource.url)?.let { Result.success(it) }
                ?: runCatching { decodeDataUrl(resource.url) }
                    .mapCatching { bytes -> keep(inlineKey(bytes), extension, bytes).getOrThrow() }
        }
    }

    private fun keep(key: String, extension: String?, bytes: ByteArray): Result<File> =
        cache.file(key, extension)?.let { Result.success(it) } ?: cache.store(key, extension, bytes)

    private fun inlineKey(bytes: ByteArray): String =
        INLINE_KEY_PREFIX + MessageDigest.getInstance("SHA-256").digest(bytes).toHex()

    // A media item of version 2 never has an `https` address (02-DESIGN §11), so the host fetches no address.
    private fun decodeDataUrl(url: String): ByteArray {
        val separator = url.indexOf(',')
        if (!url.startsWith(DATA_SCHEME) || separator < 0 || !url.substring(0, separator).endsWith(BASE64_SUFFIX)) {
            throw IOException("Only a base64 data address is a media item")
        }

        return Base64.getDecoder().decode(url.substring(separator + 1))
    }

    private fun ByteArray.toHex(): String = joinToString(separator = "") { "%02x".format(it) }

    private companion object {
        const val INLINE_KEY_PREFIX = "inline:"
        const val DATA_SCHEME = "data:"
        const val BASE64_SUFFIX = ";base64"
    }
}
