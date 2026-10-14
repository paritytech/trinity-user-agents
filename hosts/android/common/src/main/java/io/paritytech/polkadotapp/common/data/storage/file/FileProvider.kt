package io.paritytech.polkadotapp.common.data.storage.file

import android.net.Uri
import java.io.File

interface FileProvider {
    fun getFileInExternalCacheStorage(relativePath: String): File

    fun getFileInInternalCacheStorage(relativePath: String): File

    fun getFileInScopedStorage(relativePath: String): File

    fun generateTempFile(fixedPath: String? = null): File

    fun uriOf(file: File): Uri
}
