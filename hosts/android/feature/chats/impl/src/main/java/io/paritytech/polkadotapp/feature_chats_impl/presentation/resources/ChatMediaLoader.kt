package io.paritytech.polkadotapp.feature_chats_impl.presentation.resources

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatHopFiles
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ChatOutboxPreviews
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatHopFileCache
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatResourceDiskCache
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatSealedFile
import io.paritytech.polkadotapp.feature_chats_impl.data.resources.ChatSealedFileStore
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
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
     * available now, for example because the preimage lookup failed. A later call tries again.
     */
    suspend fun load(resource: ChatResourceModel, extension: String?): Result<File>
}

class RealChatMediaLoader @Inject constructor(
    private val ipfsContentLookup: IpfsContentLookup,
    private val cache: ChatResourceDiskCache,
    private val dispatchers: CoroutineDispatchers,
    private val sealedFiles: ChatSealedFileStore,
    private val hopFiles: ChatHopFiles,
    private val hopCache: ChatHopFileCache,
    private val outboxPreviews: ChatOutboxPreviews = ChatOutboxPreviews { null },
) : ChatMediaLoader {
    override suspend fun load(resource: ChatResourceModel, extension: String?): Result<File> = withContext(dispatchers.io) {
        when (resource) {
            is ChatResourceModel.Inline -> keep(inlineKey(resource.bytes.value), extension, resource.bytes.value)

            is ChatResourceModel.Preimage -> {
                val key = PREIMAGE_KEY_PREFIX + resource.key.value.toHex()
                cache.file(key, extension)?.let { return@withContext Result.success(it) }

                // A failed lookup is not kept, so that the retry of the user asks again.
                ipfsContentLookup.lookupRawHash(resource.key.value).mapCatching { bytes -> keep(key, extension, bytes).getOrThrow() }
            }

            // An opened sealed file stays out of the cache: the chain keeps its parts for about two weeks only.
            is ChatResourceModel.Sealed -> {
                val key = SEALED_KEY_PREFIX + resource.digest.value.toHex()
                sealedFiles.cache.file(key, extension)?.let { return@withContext Result.success(it) }

                val file = ChatSealedFile.read(resource) { part -> ipfsContentLookup.lookupRawHash(part).getOrNull() }
                    ?: return@withContext Result.failure(IOException("The sealed file did not open"))
                sealedFiles.cache.store(key, extension, file)
            }

            // The node drops an acked file, so it stays in the store of opened files; the ack follows the store.
            is ChatResourceModel.Hop -> {
                hopCache.file(resource, extension)?.let { return@withContext Result.success(it) }

                hopFiles.open(resource).flatMap { opened ->
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
        const val PREIMAGE_KEY_PREFIX = "preimage:"
        const val SEALED_KEY_PREFIX = "sealed:"
        const val DATA_SCHEME = "data:"
        const val BASE64_SUFFIX = ";base64"
    }
}
