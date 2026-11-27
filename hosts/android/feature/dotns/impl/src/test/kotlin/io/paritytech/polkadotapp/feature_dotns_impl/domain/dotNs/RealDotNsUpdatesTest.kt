package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHash
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsNameStorage
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.io.IOException

class RealDotNsUpdatesTest {
    private val storedNames = mutableMapOf(
        APP to StoredDotNsName(served = version("app-served"), update = null),
        WIDGET to StoredDotNsName(served = version("widget-served"), update = null),
    )
    private val deletedArchives = mutableListOf<ContentHash>()

    private val nameStorage: DotNsNameStorage = mockk<DotNsNameStorage>().also { storage ->
        every { storage.get(any()) } answers { storedNames[firstArg()] }
        every { storage.all() } answers { storedNames.toMap() }
        every { storage.putAll(any()) } answers { storedNames.putAll(firstArg()) }
    }
    private val contentStorage: DotNsContentStorage = mockk {
        coEvery { deleteContent(any()) } answers { deletedArchives += firstArg<ContentHash>() }
    }
    private val chainReader: DotNsChainReader = mockk {
        coEvery { readContentHash(APP, any()) } returns Result.success("app-latest")
        coEvery { readContentHash(WIDGET, any()) } returns Result.failure(IOException("node unreachable"))
    }
    private val contentDownloader: DotNsContentDownloader = mockk {
        coEvery { download("app-latest", any(), null) } returns Result.success(File("app-latest"))
    }

    @Test
    fun `when one name of a product cannot be updated, nothing is recorded and what the others downloaded is deleted`() = runTest {
        val updates = RealDotNsUpdates(chainReader, contentDownloader, DotNsBuilds(nameStorage, contentStorage), testDispatchers())

        val result = updates.fetchUpdates(listOf(APP, WIDGET))

        assertTrue(result.isFailure)
        assertEquals(StoredDotNsName(served = version("app-served"), update = null), storedNames[APP])
        assertEquals(listOf("app-latest"), deletedArchives)
    }

    private fun version(contentHash: ContentHash) = DotNsNameVersion(contentHash = contentHash, records = emptyMap())

    private companion object {
        const val APP = "app.kart.dot"
        const val WIDGET = "widget.kart.dot"
    }
}
