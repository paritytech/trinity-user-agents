package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameStorage
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class DotNsBuildsTest {
    private val storedNames = mutableMapOf<String, StoredDotNsName>()
    private val deletedArchives = mutableListOf<ContentHash>()
    private var keptArchives: Set<ContentHash>? = null

    private val nameStorage: DotNsNameStorage = mockk<DotNsNameStorage>().also { storage ->
        every { storage.get(any()) } answers { storedNames[firstArg()] }
        every { storage.all() } answers { storedNames.toMap() }
        every { storage.putAll(any()) } answers { storedNames.putAll(firstArg()) }
    }
    private val contentStorage: DotNsContentStorage = mockk {
        coEvery { deleteContent(any()) } answers { deletedArchives += firstArg<ContentHash>() }
        coEvery { deleteAllExcept(any()) } answers { keptArchives = firstArg() }
    }

    private val builds = DotNsBuilds(nameStorage, contentStorage)

    @Test
    fun `an applied update becomes the served build and the archive it replaced is deleted`() = runTest {
        storedNames[APP] = StoredDotNsName(served = version("old"), update = change("new"))

        builds.applyUpdates(listOf(APP))

        assertEquals(StoredDotNsName(served = version("new"), update = null), storedNames[APP])
        assertEquals(listOf("old"), deletedArchives)
    }

    @Test
    fun `an archive another name still serves is kept when an update replaces it`() = runTest {
        storedNames[APP] = StoredDotNsName(served = version("shared"), update = change("new"))
        storedNames[WIDGET] = StoredDotNsName(served = version("shared"), update = null)

        builds.applyUpdates(listOf(APP))

        assertEquals(emptyList<ContentHash>(), deletedArchives)
    }

    @Test
    fun `a check that finds the served build again drops the pending update and deletes its archive`() = runTest {
        storedNames[APP] = StoredDotNsName(served = version("served"), update = change("pending"))

        val namesWithUpdates = builds.recordLatest(mapOf(APP to change("served")))

        assertEquals(emptySet<String>(), namesWithUpdates)
        assertEquals(StoredDotNsName(served = version("served"), update = null), storedNames[APP])
        assertEquals(listOf("pending"), deletedArchives)
    }

    @Test
    fun `deleting unreferenced content keeps the served and pending archives of every name`() = runTest {
        storedNames[APP] = StoredDotNsName(served = version("served"), update = change("pending"))
        storedNames[WIDGET] = StoredDotNsName(served = version("widget"), update = null)

        builds.deleteAllUnreferencedContent()

        assertEquals(setOf("served", "pending", "widget"), keptArchives)
    }

    private fun version(contentHash: ContentHash) = DotNsNameVersion(contentHash = contentHash, records = emptyMap())

    private fun change(contentHash: ContentHash) = DotNsNameChange(contentHash = contentHash, records = emptyMap())

    private companion object {
        const val APP = "app.kart.dot"
        const val WIDGET = "widget.kart.dot"
    }
}
