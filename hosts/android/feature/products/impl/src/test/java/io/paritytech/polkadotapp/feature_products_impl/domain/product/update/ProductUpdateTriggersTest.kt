package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId

class ProductUpdateTriggersTest {
    private val appliedNames = mutableListOf<Set<String>>()

    private val productUpdater: ProductUpdater = mockk {
        every { observePendingNames() } returns MutableStateFlow(setOf(KART_APP, QUIZ_APP))
        coEvery { check(any()) } returns Result.success(Unit)
        coEvery { applyUpdates(any()) } answers {
            appliedNames += firstArg<Collection<String>>().toSet()
            emptySet()
        }
    }
    private val appLifecycleObserver: AppLifecycleObserver = mockk {
        every { subscribe() } returns emptyFlow()
    }
    private val productsInUse = ProductsInUse()

    @Test
    fun `an update waits while a page shows its name and is applied once the page closes`() = runTest {
        val pageScope = CoroutineScope(backgroundScope.coroutineContext + Job())
        val kart = ProductId.parse("kart.dot").getOrThrow()
        productsInUse.trackPage(MutableStateFlow(ShownPage(kart, KART_APP)), pageScope, reload = {})
        runCurrent()

        ProductUpdateTriggers(productUpdater, productsInUse, appLifecycleObserver).start(backgroundScope)
        runCurrent()
        pageScope.cancel()
        runCurrent()

        assertEquals(listOf(setOf(QUIZ_APP), setOf(KART_APP, QUIZ_APP)), appliedNames)
    }

    private companion object {
        const val KART_APP = "app.kart.dot"
        const val QUIZ_APP = "app.quiz.dot"
    }
}
