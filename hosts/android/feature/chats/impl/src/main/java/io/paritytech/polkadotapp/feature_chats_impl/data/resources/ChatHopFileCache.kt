package io.paritytech.polkadotapp.feature_chats_impl.data.resources

import android.webkit.MimeTypeMap
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import java.io.File
import javax.inject.Inject

/**
 * The opened HOP files, kept with the opened sealed files: the node drops a file once it is acked. The composer keeps
 * the file it uploaded here too, so this device never opens, and so never acks, its own upload.
 */
class ChatHopFileCache @Inject constructor(private val sealedFiles: ChatSealedFileStore) {
    fun file(resource: ChatResourceModel.Hop, extension: String?): File? = sealedFiles.cache.file(keyOf(resource), extension)

    fun store(resource: ChatResourceModel.Hop, extension: String?, bytes: ByteArray): Result<File> =
        sealedFiles.cache.store(keyOf(resource), extension, bytes)

    /** Keeps the bytes of a file that this device uploaded, under the extension the room screen asks with. */
    fun keepUpload(resource: ChatResourceModel.Hop, mimeType: String, bytes: ByteArray): Result<File> =
        store(resource, MimeTypeMap.getSingleton().getExtensionFromMimeType(mimeType), bytes)

    private fun keyOf(resource: ChatResourceModel.Hop): String =
        KEY_PREFIX + resource.identifier.value.joinToString(separator = "") { "%02x".format(it) }

    private companion object {
        const val KEY_PREFIX = "hop:"
    }
}
