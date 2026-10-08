package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class UninstallProductUseCaseTest {
    private val repository = mockk<ProductRepository>(relaxed = true)
    private val scheduler = mockk<ProductNotificationScheduler>()
    private val reminder = mockk<ProductGameReminder>(relaxed = true)
    private val settings = mockk<ProductRuntimeSettings>()
    private val provider = mockk<TrUAPIHostRuntimeProvider>()
    private val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)
    private val useCase = UninstallProductUseCase(repository, scheduler, reminder, settings, provider)
    private val product = ProductId.fromStoredValue("acme.dot")

    @Test
    fun `notifications grants and reminders are cleared before deleting a Rust product`() = runTest {
        coEvery { scheduler.cancelAllForProduct(product) } returns Result.success(Unit)
        every { settings.isTrUAPIRuntimeEnabled() } returns true
        coEvery { provider.runtime() } returns Result.success(runtime)
        assertEquals(Result.success(Unit), useCase(product))
        coVerifyOrder {
            scheduler.cancelAllForProduct(product)
            runtime.clearProductState(product.value)
            reminder.cancel(product)
            repository.deleteProduct(product)
        }
    }

    @Test
    fun `native product removal does not boot the Rust runtime`() = runTest {
        coEvery { scheduler.cancelAllForProduct(product) } returns Result.success(Unit)
        every { settings.isTrUAPIRuntimeEnabled() } returns false
        assertEquals(Result.success(Unit), useCase(product))
        coVerify(exactly = 0) { provider.runtime() }
        coVerifyOrder { reminder.cancel(product); repository.deleteProduct(product) }
    }

    @Test
    fun `a cleanup failure leaves the product available to retry removal`() = runTest {
        val failure = IllegalStateException("cleanup failed")
        for (failsAtGrant in listOf(false, true)) {
            every { settings.isTrUAPIRuntimeEnabled() } returns true
            coEvery { scheduler.cancelAllForProduct(product) } returns
                if (failsAtGrant) Result.success(Unit) else Result.failure(failure)
            coEvery { provider.runtime() } returns Result.failure(failure)
            assertEquals(failure, useCase(product).exceptionOrNull())
        }
        coVerify(exactly = 0) { repository.deleteProduct(any()) }
    }
}
