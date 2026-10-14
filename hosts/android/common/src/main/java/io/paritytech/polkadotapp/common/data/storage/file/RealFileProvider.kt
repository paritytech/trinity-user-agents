package io.paritytech.polkadotapp.common.data.storage.file

import android.content.Context
import android.net.Uri
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import java.io.IOException
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton
import androidx.core.content.FileProvider as AndroidFileProvider

@Singleton
internal class RealFileProvider @Inject constructor(
    @param:ApplicationContext private val context: Context
) : FileProvider {
    override fun getFileInExternalCacheStorage(relativePath: String): File {
        val cacheDir = context.externalCacheDir ?: directoryNotAvailable()

        return prepareFile(cacheDir, relativePath)
    }

    override fun getFileInInternalCacheStorage(relativePath: String): File {
        val cacheDir = context.cacheDir ?: directoryNotAvailable()

        return prepareFile(cacheDir, relativePath)
    }

    override fun getFileInScopedStorage(relativePath: String): File {
        val filesDir = context.filesDir ?: directoryNotAvailable()

        return prepareFile(filesDir, relativePath)
    }

    override fun generateTempFile(fixedPath: String?): File {
        val path = fixedPath ?: "temp/${UUID.randomUUID()}"

        return getFileInExternalCacheStorage(path)
    }

    override fun uriOf(file: File): Uri {
        return AndroidFileProvider.getUriForFile(context, "${context.packageName}.provider", file)
    }

    private fun directoryNotAvailable(): Nothing {
        throw IllegalStateException("Cache directory is unavailable")
    }

    private fun prepareFile(baseDir: File, path: String): File {
        val file = File(baseDir, path)

        file.parentFile?.let { parent ->
            if (!parent.exists() && !parent.mkdirs()) {
                throw IOException("Failed to create directory: ${parent.absolutePath}")
            }
        }

        return file
    }
}
