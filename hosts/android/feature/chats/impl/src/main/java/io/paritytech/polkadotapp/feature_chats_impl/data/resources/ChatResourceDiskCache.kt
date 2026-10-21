package io.paritytech.polkadotapp.feature_chats_impl.data.resources

import java.io.File
import java.io.IOException
import java.security.MessageDigest

/**
 * Keeps the bytes of chat resources as files in one directory (04-TESTING B6). When the files grow past [maxBytes],
 * the cache removes the files that were read longest ago. The time of the last read is the modification time of
 * the file, so the order stays after a restart of the app.
 */
class ChatResourceDiskCache(
    private val directory: File,
    private val maxBytes: Long,
    private val now: () -> Long = System::currentTimeMillis,
) {
    /** The file of [key], or null when the cache does not have it. A read makes the file the most recent. */
    @Synchronized
    fun file(key: String, extension: String?): File? {
        val file = fileFor(key, extension)
        if (!file.isFile) return null

        file.setLastModified(now())
        return file
    }

    /** Writes [bytes] for [key] and returns its file. Bytes that are larger than the bound are not kept. */
    @Synchronized
    fun store(key: String, extension: String?, bytes: ByteArray): Result<File> = runCatching {
        if (bytes.size > maxBytes) throw IOException("${bytes.size} bytes are more than the cache keeps")
        if (!directory.isDirectory && !directory.mkdirs()) throw IOException("The cache directory is not available")

        val file = fileFor(key, extension)
        val partial = File(directory, file.name + PARTIAL_SUFFIX)
        partial.writeBytes(bytes)
        if (!partial.renameTo(file)) {
            partial.delete()
            throw IOException("The cache cannot keep the file of a resource")
        }

        file.setLastModified(now())
        trim(keep = file)
        file
    }

    /** The bytes that the files of the cache hold. */
    @Synchronized
    fun sizeBytes(): Long = cachedFiles().sumOf { it.length() }

    /** Removes the files that were read longest ago until the files fit in the bound. */
    private fun trim(keep: File) {
        val files = cachedFiles().sortedBy { it.lastModified() }
        var total = files.sumOf { it.length() }

        for (file in files) {
            if (total <= maxBytes) break
            if (file == keep) continue

            val length = file.length()
            if (file.delete()) total -= length
        }
    }

    private fun cachedFiles(): List<File> =
        directory.listFiles()?.filter { it.isFile && !it.name.endsWith(PARTIAL_SUFFIX) }.orEmpty()

    private fun fileFor(key: String, extension: String?): File {
        val digest = MessageDigest.getInstance("SHA-256").digest(key.toByteArray(Charsets.UTF_8))
        val name = digest.joinToString(separator = "") { "%02x".format(it) }
        return File(directory, if (extension.isNullOrEmpty()) name else "$name.$extension")
    }

    companion object {
        /** The bound of 04-TESTING B6, the same as on iOS. */
        const val MAX_BYTES: Long = 64L * 1024 * 1024

        private const val PARTIAL_SUFFIX = ".partial"
    }
}
