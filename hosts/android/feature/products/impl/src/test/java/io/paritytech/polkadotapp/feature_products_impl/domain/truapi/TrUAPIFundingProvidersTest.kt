package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIHostRuntime
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Test

class TrUAPIFundingProvidersTest {
    private val runtime = mockk<TrUAPIHostRuntime>(relaxed = true)
    private val configuredNames = MutableSharedFlow<List<String>>()

    @Test
    fun `offers the configured providers on every remote config refresh and nothing before`() = runTest {
        TrUAPIFundingProviders(
            fundingProvidersConfig = mockk { every { publishedNames() } returns configuredNames },
        ).install(runtime, backgroundScope)
        runCurrent()

        verify(exactly = 0) { runtime.setFundingProviderConfig(any()) }

        configuredNames.emit(listOf("ramp", "other"))
        configuredNames.emit(listOf("ramp"))
        runCurrent()

        verify { runtime.setFundingProviderConfig(listOf("ramp", "other")) }
        verify { runtime.setFundingProviderConfig(listOf("ramp")) }
    }
}
