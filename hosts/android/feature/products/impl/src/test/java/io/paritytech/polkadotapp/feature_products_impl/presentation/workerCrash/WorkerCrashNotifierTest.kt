package io.paritytech.polkadotapp.feature_products_impl.presentation.workerCrash

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.notification.AppNotification
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerCrashes
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId
import io.paritytech.polkadotapp.common.R as RCommon

class WorkerCrashNotifierTest {
    private val shown = mutableListOf<AppNotification>()
    private val notifier = mockk<AppNotifier> { every { notify(any()) } answers { shown += firstArg<AppNotification>() } }
    private val contextManager = mockk<ContextManager> {
        every { applicationContext.getString(RCommon.string.worker_crashing_notice, *anyVararg()) } answers {
            "${secondArg<Array<Any>>()[0]} stopped working. Retrying…"
        }
    }
    private val resolveProduct = mockk<ResolveProductUseCase>()
    private val crashes = WorkerCrashes()
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()

    @Test
    fun `the notice names the product by its manifest`() = runTest {
        coEvery { resolveProduct.resolve(ramp) } returns Result.success(mockk<ResolvedProduct> { every { product.name } returns "Ramp" })
        val showing = launch { WorkerCrashNotifier(crashes, resolveProduct, notifier, contextManager).showNotices() }
        advanceUntilIdle()

        crashes.report(ramp)
        advanceUntilIdle()
        showing.cancel()

        assertEquals(listOf(AppNotification.Error("Ramp stopped working. Retrying…")), shown)
    }

    @Test
    fun `a name that cannot be resolved falls back to the product id`() = runTest {
        coEvery { resolveProduct.resolve(ramp) } returns Result.failure(IllegalStateException("no manifest"))
        val showing = launch { WorkerCrashNotifier(crashes, resolveProduct, notifier, contextManager).showNotices() }
        advanceUntilIdle()

        crashes.report(ramp)
        advanceUntilIdle()
        showing.cancel()

        assertEquals(listOf(AppNotification.Error("ramp.dot stopped working. Retrying…")), shown)
    }
}
