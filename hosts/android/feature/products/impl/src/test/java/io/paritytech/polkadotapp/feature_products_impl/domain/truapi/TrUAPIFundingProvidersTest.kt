package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Test
import uniffi.truapi.FundingProviderApp
import uniffi.truapi.FundingProviderEntry
import uniffi.truapi.FundingProviderOrigin
import uniffi.truapi.ProductId

class TrUAPIFundingProvidersTest {
    private val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)
    private val configuredNames = MutableSharedFlow<List<String>>()

    @Test
    fun `offers the bundled providers at once and the configured ones on every remote config refresh`() = runTest {
        val bundled = listOf(bundled("fundtest.dot"))
        install(bundled)
        runCurrent()

        verify(exactly = 1) { runtime.setFundingProviderConfig(emptyList(), bundled) }

        configuredNames.emit(listOf("ramp.dot", "other.dot"))
        configuredNames.emit(listOf("ramp.dot"))
        runCurrent()

        verify { runtime.setFundingProviderConfig(listOf("ramp.dot", "other.dot"), bundled) }
        verify { runtime.setFundingProviderConfig(listOf("ramp.dot"), bundled) }
    }

    @Test
    fun `offers nothing before remote config when no provider is bundled`() = runTest {
        install(emptyList())
        runCurrent()

        verify(exactly = 0) { runtime.setFundingProviderConfig(any(), any()) }

        configuredNames.emit(listOf("ramp.dot"))
        runCurrent()

        verify { runtime.setFundingProviderConfig(listOf("ramp.dot"), emptyList()) }
    }

    private fun TestScope.install(bundled: List<FundingProviderEntry>) {
        TrUAPIFundingProviders(
            bundledProducts = mockk { every { fundingProviders() } returns bundled },
            fundingProvidersConfig = mockk { every { publishedNames() } returns configuredNames },
        ).install(runtime, backgroundScope)
    }

    private fun bundled(name: String) = FundingProviderEntry(
        productId = ProductId.parse(name).getOrThrow(),
        workerManifest = "{}",
        rootManifest = null,
        origin = FundingProviderOrigin.BUNDLED,
        app = FundingProviderApp.SHIPPED,
    )
}
