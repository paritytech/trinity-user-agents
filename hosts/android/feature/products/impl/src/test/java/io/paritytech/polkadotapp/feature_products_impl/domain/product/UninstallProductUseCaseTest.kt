package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.repository.ProductRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.ProductNotificationScheduler
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.inOrder
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class UninstallProductUseCaseTest {
    private val productRepository: ProductRepository = mock()
    private val scheduler: ProductNotificationScheduler = mock()
    private val productGameReminder: ProductGameReminder = mock()
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

        val order = inOrder(productGameReminder, productRepository)
        order.verify(productGameReminder).cancel(productId)
        order.verify(productRepository).deleteProduct(productId)
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
        whenever(scheduler.cancelAllForProduct(productId)).thenReturn(Result.success(Unit))
    }

    private suspend fun givenSchedulerCancelFails(failure: Exception) {
        whenever(scheduler.cancelAllForProduct(productId)).thenReturn(Result.failure(failure))
    }

    private suspend fun uninstallProduct(): Result<Unit> = useCase(productId)

    private suspend fun assertProductDeletedAfterAlarmsCancelled() {
        val order = inOrder(scheduler, productRepository)
        order.verify(scheduler).cancelAllForProduct(productId)
        order.verify(productRepository).deleteProduct(productId)
    }

    private suspend fun assertProductNotDeleted() {
        verify(productRepository, never()).deleteProduct(productId)
    }
}
