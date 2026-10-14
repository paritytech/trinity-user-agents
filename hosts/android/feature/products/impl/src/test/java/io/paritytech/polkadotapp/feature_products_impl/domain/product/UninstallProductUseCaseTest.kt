package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class UninstallProductUseCaseTest {
    private val productRepository: ProductRepository = mockk(relaxUnitFun = true)
    private val scheduler: ProductNotificationScheduler = mockk()
    private val productGameReminder: ProductGameReminder = mockk(relaxUnitFun = true)
    private val useCase = UninstallProductUseCase(productRepository, scheduler, productGameReminder)

    private val productId = ProductId.fromStoredValue("acme.dot")

    @Test
    fun `cancels scheduled notifications before deleting product`() = runBlocking {
        givenSchedulerCancelsSuccessfully()

        val result = uninstallProduct()

        assertTrue(result.isSuccess)
        assertProductDeletedAfterAlarmsCancelled()
    }

    @Test
    fun `cancels the game reminder before deleting product`() = runBlocking<Unit> {
        givenSchedulerCancelsSuccessfully()

        uninstallProduct()

        coVerifyOrder {
            productGameReminder.cancel(productId)
            productRepository.deleteProduct(productId)
        }
        coVerify(exactly = 1) { productGameReminder.cancel(productId) }
        coVerify(exactly = 1) { productRepository.deleteProduct(productId) }
    }

    @Test
    fun `does not delete product when scheduler cancel fails`() = runBlocking {
        val failure = IllegalStateException("alarm cancel failed")
        givenSchedulerCancelFails(failure)

        val result = uninstallProduct()

        assertTrue(result.isFailure)
        assertEquals(failure, result.exceptionOrNull())
        assertProductNotDeleted()
    }

    private suspend fun givenSchedulerCancelsSuccessfully() {
        coEvery { scheduler.cancelAllForProduct(productId) } returns Result.success(Unit)
    }

    private suspend fun givenSchedulerCancelFails(failure: Exception) {
        coEvery { scheduler.cancelAllForProduct(productId) } returns Result.failure(failure)
    }

    private suspend fun uninstallProduct(): Result<Unit> = useCase(productId)

    private suspend fun assertProductDeletedAfterAlarmsCancelled() {
        coVerifyOrder {
            scheduler.cancelAllForProduct(productId)
            productRepository.deleteProduct(productId)
        }
        coVerify(exactly = 1) { scheduler.cancelAllForProduct(productId) }
        coVerify(exactly = 1) { productRepository.deleteProduct(productId) }
    }

    private suspend fun assertProductNotDeleted() {
        coVerify(exactly = 0) { productRepository.deleteProduct(productId) }
    }
}
