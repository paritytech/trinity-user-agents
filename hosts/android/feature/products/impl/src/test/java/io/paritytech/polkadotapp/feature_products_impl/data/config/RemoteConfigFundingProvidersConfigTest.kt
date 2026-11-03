package io.paritytech.polkadotapp.feature_products_impl.data.config

import com.google.gson.Gson
import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId

class RemoteConfigFundingProvidersConfigTest {
    private val remoteConfig = mockk<RemoteConfigService>()
    private val config = RemoteConfigFundingProvidersConfig(remoteConfig)

    @Test
    fun `lists the configured providers in order`() = runTest {
        remoteConfigReturns("""{ "onrampUrl": "getcash.dot", "providers": ["ramp.dot", "other.dot"] }""")

        assertEquals(ids("ramp.dot", "other.dot"), config.getFundingProviderIds().getOrThrow())
    }

    @Test
    fun `drops repeated providers, keeping the first position`() = runTest {
        remoteConfigReturns("""{ "providers": ["ramp.dot", "other.dot", "ramp.dot"] }""")

        assertEquals(ids("ramp.dot", "other.dot"), config.getFundingProviderIds().getOrThrow())
    }

    @Test
    fun `skips names that are not product ids and keeps the rest`() = runTest {
        remoteConfigReturns("""{ "providers": ["not a product!", null, "ramp.dot"] }""")

        assertEquals(ids("ramp.dot"), config.getFundingProviderIds().getOrThrow())
    }

    @Test
    fun `lists none when the config carries no providers`() = runTest {
        remoteConfigReturns("""{ "onrampUrl": "getcash.dot", "offrampUrl": "https://getcash.dot/offramp" }""")

        assertEquals(emptyList<ProductId>(), config.getFundingProviderIds().getOrThrow())
    }

    @Test
    fun `fails when remote config cannot be read`() = runTest {
        coEvery { remoteConfig.getSyncedJsonObject(any(), any<Class<*>>()) } returns
            Result.failure(IllegalStateException("offline"))

        assertTrue(config.getFundingProviderIds().isFailure)
    }

    // Parses with Gson as the real service does, so the remote model's nullability is exercised.
    private fun remoteConfigReturns(json: String) {
        coEvery { remoteConfig.getSyncedJsonObject("funding_config", any<Class<Any>>()) } answers {
            Result.success(Gson().fromJson(json, secondArg<Class<Any>>()))
        }
    }

    private fun ids(vararg names: String) = names.map { ProductId.parse(it).getOrThrow() }
}
