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
        storage.save(PEOPLE, "people-1", stack = null)
        storage.save(ASSET_HUB, "asset-hub-1", stack = null)
        storage.save(PEOPLE, "people-2", stack = null)

        assertEquals(listOf("people-2", "asset-hub-1"), listOf(storage.load(PEOPLE, stack = null), storage.load(ASSET_HUB, stack = null)))
    }

    @Test
    fun `a chain never saved starts without state`() = runBlocking {
        assertNull(storage.load(PEOPLE, stack = null))
    }

    // A process killed mid-write must not leave a truncated blob that smoldot then refuses or,
    // worse, that the next start reads as the chain's state.
    @Test
    fun `a save that fails part way keeps the previous state`() = runBlocking {
        storage.save(PEOPLE, "people-1", stack = null)
        File(folder.root, "light-client/${PEOPLE.toHexString()}.tmp").mkdirs()

        val failed = runCatching { storage.save(PEOPLE, "people-2", stack = null) }

        assertTrue("expected the save to fail", failed.isFailure)
        assertEquals("people-1", storage.load(PEOPLE, stack = null))
    }

    // The provider keys state by stack as well as by chain: a stack's state must never be read
    // back as another stack's, or as the state of a provider built without one.
    @Test
    fun `the same chain keeps separate state per stack`() = runBlocking {
        storage.save(PEOPLE, "no-stack", stack = null)
        storage.save(PEOPLE, "stack-a", stack = "https://stacks.example/a?v=1")
        storage.save(PEOPLE, "stack-b", stack = "https://stacks.example/b")

        assertEquals(
            listOf("no-stack", "stack-a", "stack-b"),
            listOf(
                storage.load(PEOPLE, stack = null),
                storage.load(PEOPLE, stack = "https://stacks.example/a?v=1"),
                storage.load(PEOPLE, stack = "https://stacks.example/b"),
            ),
        )
    }

    // Turning the light client off would otherwise leave megabytes per chain that nothing reads.
    @Test
    fun `clearing removes every chain's state and its files`() = runBlocking {
        storage.save(PEOPLE, "people-1", stack = null)
        storage.save(ASSET_HUB, "asset-hub-1", stack = null)

        storage.clear()

        val remaining = listOf(storage.load(PEOPLE, stack = null), storage.load(ASSET_HUB, stack = null), File(folder.root, "light-client").exists())
        assertEquals(listOf(null, null, false), remaining)
    }

    private companion object {
        val PEOPLE = ByteArray(32) { 1 }
        val ASSET_HUB = ByteArray(32) { 2 }
    }
}
