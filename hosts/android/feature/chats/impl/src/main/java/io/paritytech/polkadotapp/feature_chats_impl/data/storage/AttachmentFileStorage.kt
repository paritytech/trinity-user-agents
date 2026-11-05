package io.paritytech.polkadotapp.feature_chats_impl.data.storage

import android.net.Uri
import android.webkit.MimeTypeMap
import io.paritytech.polkadotapp.common.data.storage.file.ContentResolver
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.readNBytesCompat
import io.paritytech.polkadotapp.common.utils.skipNBytesCompat
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopService
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileNotFoundException
import javax.inject.Inject

class AttachmentFileStorage @Inject constructor(
    private val contentResolver: ContentResolver,
    private val fileProvider: FileProvider,
    private val coroutineDispatchers: CoroutineDispatchers
) {
    suspend fun copyToLocalStorage(contentUri: Uri, mimeType: String): Uri = withContext(coroutineDispatchers.io) {
        val extension = MimeTypeMap.getSingleton().getExtensionFromMimeType(mimeType)
        val localFile = fileProvider.getFileInScopedStorage("uploads/${System.currentTimeMillis()}.$extension")

        contentResolver.openInputStream(contentUri)?.use { input ->
            localFile.outputStream().use { output ->
                input.copyTo(output)
            }
        } ?: throw FileNotFoundException("Cannot open file: $contentUri")

        fileProvider.uriOf(localFile)
    }

    suspend fun readFileBytes(uri: Uri): ByteArray = withContext(coroutineDispatchers.io) {
        contentResolver.openInputStream(uri)?.use { it.readBytes() }
            ?: throw FileNotFoundException("Cannot open file: $uri")
    }

    suspend fun sizeOf(uri: Uri): InformationSize = withContext(coroutineDispatchers.io) {
        contentResolver.querySize(uri) ?: throw FileNotFoundException("Cannot read size of file: $uri")
    }

    fun readChunks(uri: Uri, fromChunk: Int): Flow<ByteArray> = flow {
        val input = withContext(coroutineDispatchers.io) { contentResolver.openInputStream(uri) }
            ?: throw FileNotFoundException("Cannot open file: $uri")

        input.use {
            withContext(coroutineDispatchers.io) { input.skipNBytesCompat(fromChunk.toLong() * HopService.CHUNK_SIZE_BYTES) }

            while (true) {
                val chunk = withContext(coroutineDispatchers.io) { input.readNBytesCompat(HopService.CHUNK_SIZE_BYTES) }
                if (chunk.isEmpty()) break

                emit(chunk)
            }
        }
    }

    fun createDownloadFile(mimeType: String): File {
        val extension = MimeTypeMap.getSingleton().getExtensionFromMimeType(mimeType)
        return fileProvider.getFileInScopedStorage("downloads/${System.currentTimeMillis()}.$extension")
    }
}

class FileTooLargeException(
    val actualSize: InformationSize,
    val maxSize: InformationSize
) : IllegalArgumentException("File size $actualSize exceeds maximum allowed $maxSize")
