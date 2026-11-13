package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.notification.AppNotification
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId
import io.paritytech.polkadotapp.common.R as RCommon

class WorkerCrashNoticeTest {
    private val shown = mutableListOf<AppNotification>()
    private val notifier = mockk<AppNotifier> { every { notify(any()) } answers { shown += firstArg<AppNotification>() } }
    private val contextManager = mockk<ContextManager> {
        every { applicationContext.getString(RCommon.string.worker_crashing_notice, *anyVararg()) } answers {
            "${secondArg<Array<Any>>()[0]} stopped working. Retrying…"
        }
    }
    private val resolveProduct = mockk<ResolveProductUseCase>()
    private val ramp = ProductId.parse("ramp.dot").getOrThrow()

    @Test
    fun `the notice names the product by its manifest`() = runTest {
        coEvery { resolveProduct.resolve(ramp) } returns Result.success(mockk<ResolvedProduct> { every { product.name } returns "Ramp" })

        notice().onCrashing(ramp)
        advanceUntilIdle()

        assertEquals(listOf(AppNotification.Error("Ramp stopped working. Retrying…")), shown)
    }

    @Test
    fun `a name that cannot be resolved falls back to the product id`() = runTest {
        coEvery { resolveProduct.resolve(ramp) } returns Result.failure(IllegalStateException("no manifest"))

        notice().onCrashing(ramp)
        advanceUntilIdle()

        assertEquals(listOf(AppNotification.Error("ramp.dot stopped working. Retrying…")), shown)
    }

    private fun TestScope.notice() = WorkerCrashNotice({ resolveProduct }, notifier, contextManager, testDispatchers())
}
