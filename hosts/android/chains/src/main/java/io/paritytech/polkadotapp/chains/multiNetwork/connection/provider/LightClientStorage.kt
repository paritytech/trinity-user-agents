package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.truapi_provider.StorageClient
import java.io.File
import java.net.URLEncoder
import javax.inject.Inject

/** Keeps the light client's finalized state in no-backup storage, one file per chain. */
class LightClientStorage @Inject constructor(
    @param:ApplicationContext context: Context,
) : StorageClient {
    private val directory = File(context.noBackupFilesDir, "light-client")

    override suspend fun load(genesisHash: ByteArray, stack: String?): String? = withContext(Dispatchers.IO) {
        file(genesisHash, stack).takeIf(File::exists)?.readText()
    }

    // Written aside and renamed, so a process killed mid-write leaves the previous state intact.
    override suspend fun save(genesisHash: ByteArray, blob: String, stack: String?) = withContext(Dispatchers.IO) {
        val target = file(genesisHash, stack)
        target.parentFile?.mkdirs()
        val written = File(target.parentFile, "${target.name}.tmp").apply { writeText(blob) }
        check(written.renameTo(target)) { "could not store the light client state for ${target.name}" }
    }

    /** Removes every chain's stored state, for when the light client is off and will not read it. */
    suspend fun clear() {
        withContext(Dispatchers.IO) { directory.deleteRecursively() }
    }

    // A provider built without a stack keeps the top-level layout; a stack's chains get a directory of
    // their own, since the same genesis on another stack is another blob.
    private fun file(genesisHash: ByteArray, stack: String?): File {
        val parent = stack?.let { File(directory, URLEncoder.encode(it, Charsets.UTF_8.name())) } ?: directory
        return File(parent, genesisHash.toHexString())
    }
}
