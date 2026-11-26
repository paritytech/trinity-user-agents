package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.InputStream
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import javax.inject.Inject

class RealDotNsContentStorage @Inject constructor(
    @param:ApplicationContext private val context: Context
) : DotNsContentStorage {
    private val baseDir: File
        get() = File(context.filesDir, DOTNS_DIR)

    override suspend fun saveContent(contentHash: String, files: Map<String, ByteArray>) {
        withContext(Dispatchers.IO) {
            val contentPath = File(baseDir, contentHash)
            val isSingleRootFile = files.size == 1 && files.keys.single().removePrefix("/").isEmpty()

            if (isSingleRootFile) {
                // Single root file archive — save as a plain file
                baseDir.mkdirs()
                contentPath.writeBytes(files.values.single())
            } else {
                // Multi-file archive — save as a directory
                contentPath.mkdirs()

                files.forEach { (path, data) ->
                    val relativePath = path.removePrefix("/")
                    if (relativePath.isEmpty()) return@forEach

                    val file = File(contentPath, relativePath)
                    file.parentFile?.mkdirs()
                    if (!file.isDirectory) {
                        file.writeBytes(data)
                    }
                }
            }
        }
    }

    override suspend fun saveContentStreaming(contentHash: ContentHash, write: (CarContentWriter) -> Unit) {
        withContext(Dispatchers.IO) {
            val unpackingPath = File(baseDir, "$contentHash$UNPACKING_SUFFIX")
            unpackingPath.deleteRecursively()

            try {
                write { path, content -> writeEntry(unpackingPath, path, content) }
                Files.move(unpackingPath.toPath(), File(baseDir, contentHash).toPath(), StandardCopyOption.ATOMIC_MOVE)
            } catch (e: Throwable) {
                unpackingPath.deleteRecursively()
                throw e
            }
        }
    }

    private fun writeEntry(contentPath: File, path: String, content: InputStream) {
        val relativePath = path.removePrefix("/")

        if (relativePath.isEmpty()) {
            // Single root file archive — save as a plain file
            baseDir.mkdirs()
            contentPath.outputStream().use { content.copyTo(it) }
        } else {
            // Multi-file archive — save as a directory
            contentPath.mkdirs()

            val file = File(contentPath, relativePath)
            file.parentFile?.mkdirs()
            if (!file.isDirectory) {
                file.outputStream().use { content.copyTo(it) }
            }
        }
    }

    override fun getContentDirectory(contentHash: String): File? {
        val path = File(baseDir, contentHash)
        return if (path.exists()) path else null
    }

    override fun contentExists(contentHash: String): Boolean {
        return File(baseDir, contentHash).exists()
    }

    override suspend fun deleteContent(contentHash: String) {
        withContext(Dispatchers.IO) {
            File(baseDir, contentHash).deleteRecursively()
        }
    }

    override suspend fun deleteAll() {
        withContext(Dispatchers.IO) {
            baseDir.deleteRecursively()
        }
    }

    override suspend fun deleteAllExcept(contentHashes: Set<ContentHash>) {
        withContext(Dispatchers.IO) {
            baseDir.listFiles()?.filterNot { it.name in contentHashes }?.forEach { it.deleteRecursively() }
        }
    }

    companion object {
        private const val DOTNS_DIR = "dotns_content"
        private const val UNPACKING_SUFFIX = ".unpacking"
    }
}
