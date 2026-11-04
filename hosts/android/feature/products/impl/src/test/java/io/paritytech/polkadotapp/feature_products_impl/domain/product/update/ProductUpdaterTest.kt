package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.mockk.MockKMatcherScope
import io.mockk.coEvery
import io.mockk.coJustRun
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
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
import uniffi.truapi.ProductId

private val TLD = DotNsTld.parse("paseo")!!
private val GETCASH = ProductId.parse("getcash.paseo").getOrThrow()
private val SWAP = ProductId.parse("swap.paseo").getOrThrow()

private const val GETCASH_APP = "app.getcash.paseo"
private const val GETCASH_WORKER = "worker.getcash.paseo"
private const val SWAP_APP = "swap.paseo"

class ProductUpdaterTest {
    private val pendingNames = MutableStateFlow<Set<String>>(emptySet())
    private val dotNsResolver: DotNsResolver = mockk {
        every { namesOnDevice() } returns setOf(GETCASH_APP, GETCASH_WORKER, SWAP_APP)
        every { observePendingUpdates() } returns pendingNames
        coEvery { fetchUpdates(any()) } returns Result.success(false)
        coJustRun { applyUpdates(any()) }
    }
    private val resolveProductUseCase: ResolveProductUseCase = mockk(relaxUnitFun = true)
    private val dotNsTldProvider: DotNsTldProvider = mockk {
        coEvery { getTld() } returns Result.success(TLD)
    }
    private val productsInUse = ProductsInUse()

    private val updater = ProductUpdater(dotNsResolver, productsInUse, dotNsTldProvider, resolveProductUseCase)

    @Test
    fun `a product coming into use is checked for a new version of all its names together`() = runTest {
        updater.start(backgroundScope)

        productsInUse.hold(GETCASH, CoroutineScope(Job()))
        runCurrent()

        coVerify(exactly = 1) { dotNsResolver.fetchUpdates(any()) }
        coVerify { dotNsResolver.fetchUpdates(namesOf(GETCASH_APP, GETCASH_WORKER)) }
    }

    @Test
    fun `a downloaded version waits until nothing of the product is running`() = runTest {
        updater.start(backgroundScope)
        val page = CoroutineScope(Job())
        productsInUse.hold(GETCASH, page)

        pendingNames.value = setOf(GETCASH_APP)
        runCurrent()
        coVerify(exactly = 0) { dotNsResolver.applyUpdates(any()) }

        page.cancel()
        runCurrent()
        coVerify { dotNsResolver.applyUpdates(namesOf(GETCASH_APP)) }
    }

    @Test
    fun `a downloaded version of a product not running is applied at once`() = runTest {
        updater.start(backgroundScope)

        pendingNames.value = setOf(SWAP_APP)
        runCurrent()

        coVerify(exactly = 1) { dotNsResolver.applyUpdates(any()) }
        coVerify { dotNsResolver.applyUpdates(namesOf(SWAP_APP)) }
    }

    @Test
    fun `applying a version drops the manifest read from the one it replaces`() = runTest {
        pendingNames.value = setOf(GETCASH_APP, GETCASH_WORKER)

        updater.apply(GETCASH)

        coVerifyOrder {
            dotNsResolver.applyUpdates(namesOf(GETCASH_APP, GETCASH_WORKER))
            resolveProductUseCase.invalidate(GETCASH)
        }
    }

    @Test
    fun `applying a page leaves the product's other names on their current build`() = runTest {
        pendingNames.value = setOf(GETCASH_APP, GETCASH_WORKER)

        updater.applyPage(GETCASH, GETCASH_APP)

        coVerifyOrder {
            dotNsResolver.applyUpdates(namesOf(GETCASH_APP))
            resolveProductUseCase.invalidate(GETCASH)
        }
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

        coVerify(exactly = 0) { dotNsResolver.applyUpdates(any()) }
    }

    @Test
    fun `pending builds are reported by product`() = runTest {
        pendingNames.value = setOf(GETCASH_APP, SWAP_APP)

        assertEquals(setOf(GETCASH, SWAP), updater.observePending().first())
    }

    @Test
    fun `checking everything checks each product on the device with its names together`() = runTest {
        updater.checkAll()

        coVerify(exactly = 2) { dotNsResolver.fetchUpdates(any()) }
        coVerify { dotNsResolver.fetchUpdates(namesOf(GETCASH_APP, GETCASH_WORKER)) }
        coVerify { dotNsResolver.fetchUpdates(namesOf(SWAP_APP)) }
    }

    @Test
    fun `a product chain no longer answers for does not fail the run`() = runTest {
        coEvery { dotNsResolver.fetchUpdates(namesOf(SWAP_APP)) } returns Result.failure(IllegalStateException("no content"))

        assertEquals(Result.success(Unit), updater.checkAll())
    }

    @Test
    fun `a run that reaches nothing fails`() = runTest {
        coEvery { dotNsResolver.fetchUpdates(any()) } returns Result.failure(IllegalStateException("no node"))

        assertTrue(updater.checkAll().isFailure)
    }

    private fun MockKMatcherScope.namesOf(vararg names: String): Collection<String> = match { it.toSet() == names.toSet() }
}
