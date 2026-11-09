package io.paritytech.polkadotapp.feature_products_impl.data.config

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

// The core reads the names, dropping repeats and the ones that are not product ids; this only takes them out of the JSON.
class RemoteConfigFundingProvidersConfigTest {
    private val remoteConfig = mockk<RemoteConfigService>()
    private val config = RemoteConfigFundingProvidersConfig(remoteConfig)

    @Test
    fun `lists the configured names in order on every refresh`() = runTest {
        remoteConfigEmits(
            """{ "onrampUrl": "legacy.dot", "providers": ["ramp.dot", "other.dot"] }""",
            """{ "providers": ["ramp.dot"] }""",
        )

        assertEquals(listOf(listOf("ramp.dot", "other.dot"), listOf("ramp.dot")), config.publishedNames().toList())
    }

    @Test
    fun `leaves out null names and lists none without providers`() = runTest {
        remoteConfigEmits("""{ "providers": [null, "ramp.dot"] }""", """{ "onrampUrl": "legacy.dot" }""")

        assertEquals(listOf(listOf("ramp.dot"), emptyList()), config.publishedNames().toList())
    }

    @Test
    fun `skips a value that does not parse`() = runTest {
        remoteConfigEmits("not json", """{ "providers": ["ramp.dot"] }""")

        assertEquals(listOf(listOf("ramp.dot")), config.publishedNames().toList())
    }

    private fun remoteConfigEmits(vararg values: String) {
        every { remoteConfig.observeString("funding_config") } returns flowOf(*values)
    }
}
