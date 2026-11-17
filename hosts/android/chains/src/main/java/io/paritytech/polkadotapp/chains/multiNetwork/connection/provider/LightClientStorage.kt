package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.truapi_provider.StorageClient
import java.io.File
import javax.inject.Inject

/** Keeps the light client's finalized state in no-backup storage, one file per chain. */
class LightClientStorage @Inject constructor(
    @param:ApplicationContext context: Context,
) : StorageClient {
    private val directory = File(context.noBackupFilesDir, "light-client")

    override suspend fun load(genesisHash: ByteArray): String? = withContext(Dispatchers.IO) {
        file(genesisHash).takeIf(File::exists)?.readText()
    }

    // Written aside and renamed, so a process killed mid-write leaves the previous state intact.
    override suspend fun save(genesisHash: ByteArray, blob: String) = withContext(Dispatchers.IO) {
        directory.mkdirs()
        val target = file(genesisHash)
        val written = File(directory, "${target.name}.tmp").apply { writeText(blob) }
        check(written.renameTo(target)) { "could not store the light client state for ${target.name}" }
    }

    /** Removes every chain's stored state, for when the light client is off and will not read it. */
    suspend fun clear() {
        withContext(Dispatchers.IO) { directory.deleteRecursively() }
    }

    private fun file(genesisHash: ByteArray) = File(directory, genesisHash.toHexString())
}
