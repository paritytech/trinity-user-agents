package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.operation.ProductOperationService
import kotlinx.coroutines.test.runTest
import org.junit.Test

class ProductWorkerInitializerTest {
    @Test
    fun `a legacy operation cannot restart a worker in the Rust runtime`() = runTest {
        val operations = mockk<ProductOperationService>(relaxed = true)
        val settings = mockk<ProductRuntimeSettings>()
        val initializer = ProductWorkerInitializer(operations, settings)

        every { settings.isTrUAPIRuntimeEnabled() } returns true
        with(ComputationalScope(this)) { initializer.initialize().getOrThrow() }
        testScheduler.runCurrent()
        coVerify(exactly = 0) { operations.resumeOpenOperations() }

        every { settings.isTrUAPIRuntimeEnabled() } returns false
        with(ComputationalScope(this)) { initializer.initialize().getOrThrow() }
        testScheduler.runCurrent()
        coVerify(exactly = 1) { operations.resumeOpenOperations() }
    }
}
