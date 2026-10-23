package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.mockk.coEvery
import io.mockk.coJustRun
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private val TLD = DotNsTld.parse("paseo")!!
private val GETCASH = ProductId.fromStoredValue("getcash.paseo")
private val SWAP = ProductId.fromStoredValue("swap.paseo")

private const val GETCASH_APP = "app.getcash.paseo"
private const val GETCASH_WORKER = "worker.getcash.paseo"
private const val SWAP_APP = "swap.paseo"

class ProductUpdaterTest {
    private val pendingNames = MutableStateFlow<Set<String>>(emptySet())
    private val dotNsResolver: DotNsResolver = mockk {
        every { namesOnDevice() } returns setOf(GETCASH_APP, GETCASH_WORKER, SWAP_APP)
        every { observePendingUpdates() } returns pendingNames
        coEvery { fetchUpdate(any()) } returns Result.success(false)
        coJustRun { applyUpdate(any()) }
    }
    private val dotNsTldProvider: DotNsTldProvider = mockk {
        coEvery { getTld() } returns Result.success(TLD)
    }
    private val productsInUse = ProductsInUse()

    private val updater = ProductUpdater(dotNsResolver, productsInUse, dotNsTldProvider)

    // The policy runs in the test's background scope, which advanceUntilIdle leaves alone; runCurrent drives it.

    // The check rides on the product being opened, so a build published since the last visit is found without a background run.
    @Test
    fun `a product coming into use is checked for a new build of each of its names`() = runTest {
        updater.start(backgroundScope)

        productsInUse.hold(GETCASH, CoroutineScope(Job()))
        runCurrent()

        coVerify { dotNsResolver.fetchUpdate(GETCASH_APP) }
        coVerify { dotNsResolver.fetchUpdate(GETCASH_WORKER) }
        coVerify(exactly = 0) { dotNsResolver.fetchUpdate(SWAP_APP) }
    }

    // Switching the build under a running page or worker would break it mid-flow.
    @Test
    fun `a downloaded build waits until nothing of the product is running`() = runTest {
        updater.start(backgroundScope)
        val page = CoroutineScope(Job())
        productsInUse.hold(GETCASH, page)

        pendingNames.value = setOf(GETCASH_APP)
        runCurrent()
        coVerify(exactly = 0) { dotNsResolver.applyUpdate(any()) }

        page.cancel()
        runCurrent()
        coVerify { dotNsResolver.applyUpdate(GETCASH_APP) }
        coVerify { dotNsResolver.applyUpdate(GETCASH_WORKER) }
    }

    @Test
    fun `a downloaded build of a product not running is applied at once`() = runTest {
        updater.start(backgroundScope)

        pendingNames.value = setOf(SWAP_APP)
        runCurrent()

        coVerify { dotNsResolver.applyUpdate(SWAP_APP) }
        coVerify(exactly = 0) { dotNsResolver.applyUpdate(GETCASH_APP) }
    }

    @Test
    fun `a page of the product keeps another page's close from applying the build`() = runTest {
        updater.start(backgroundScope)
        val tab = CoroutineScope(Job())
        val worker = CoroutineScope(Job())
        productsInUse.hold(GETCASH, tab)
        productsInUse.hold(GETCASH, worker)
        pendingNames.value = setOf(GETCASH_APP)

        tab.cancel()
        runCurrent()

        coVerify(exactly = 0) { dotNsResolver.applyUpdate(any()) }
    }

    @Test
    fun `pending builds are reported by product`() = runTest {
        pendingNames.value = setOf(GETCASH_APP, SWAP_APP)

        assertEquals(setOf(GETCASH, SWAP), updater.observePending().first())
    }

    @Test
    fun `checking everything covers each name on the device`() = runTest {
        updater.checkAll()

        coVerify { dotNsResolver.fetchUpdate(GETCASH_APP) }
        coVerify { dotNsResolver.fetchUpdate(GETCASH_WORKER) }
        coVerify { dotNsResolver.fetchUpdate(SWAP_APP) }
    }

    // A product taken off dotNS stays on the device; its name failing every run must not turn the run into retries.
    @Test
    fun `a name chain no longer answers for does not fail the run`() = runTest {
        coEvery { dotNsResolver.fetchUpdate(SWAP_APP) } returns Result.failure(IllegalStateException("no content"))

        assertEquals(Result.success(Unit), updater.checkAll())
    }

    @Test
    fun `a run that reaches nothing fails`() = runTest {
        coEvery { dotNsResolver.fetchUpdate(any()) } returns Result.failure(IllegalStateException("no node"))

        assertTrue(updater.checkAll().isFailure)
    }
}
