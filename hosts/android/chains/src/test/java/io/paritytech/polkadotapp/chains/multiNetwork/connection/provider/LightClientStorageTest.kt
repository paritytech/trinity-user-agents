package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import android.content.Context
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class LightClientStorageTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val storage by lazy {
        LightClientStorage(mockk<Context> { every { noBackupFilesDir } returns folder.root })
    }

    @Test
    fun `each chain loads back the state it saved last`() = runBlocking {
        storage.save(PEOPLE, "people-1")
        storage.save(ASSET_HUB, "asset-hub-1")
        storage.save(PEOPLE, "people-2")

        assertEquals(listOf("people-2", "asset-hub-1"), listOf(storage.load(PEOPLE), storage.load(ASSET_HUB)))
    }

    @Test
    fun `a chain never saved starts without state`() = runBlocking {
        assertNull(storage.load(PEOPLE))
    }

    // A process killed mid-write must not leave a truncated blob that smoldot then refuses or,
    // worse, that the next start reads as the chain's state.
    @Test
    fun `a save that fails part way keeps the previous state`() = runBlocking {
        storage.save(PEOPLE, "people-1")
        File(folder.root, "light-client/${PEOPLE.toHexString()}.tmp").mkdirs()

        val failed = runCatching { storage.save(PEOPLE, "people-2") }

        assertTrue("expected the save to fail", failed.isFailure)
        assertEquals("people-1", storage.load(PEOPLE))
    }

    // Turning the light client off would otherwise leave megabytes per chain that nothing reads.
    @Test
    fun `clearing removes every chain's state and its files`() = runBlocking {
        storage.save(PEOPLE, "people-1")
        storage.save(ASSET_HUB, "asset-hub-1")

        storage.clear()

        val remaining = listOf(storage.load(PEOPLE), storage.load(ASSET_HUB), File(folder.root, "light-client").exists())
        assertEquals(listOf(null, null, false), remaining)
    }

    private companion object {
        val PEOPLE = ByteArray(32) { 1 }
        val ASSET_HUB = ByteArray(32) { 2 }
    }
}
