package io.paritytech.polkadotapp.feature_products_impl.data.config

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsDebugSettings
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.MaterialsOverride
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteConfigMaterialsSwitchTest {
    private val remoteConfig = mockk<RemoteConfigService>()

    private fun switch(override: MaterialsOverride) = RemoteConfigMaterialsSwitch(
        remoteConfig,
        object : MaterialsDebugSettings {
            override fun override() = override

            override fun setOverride(override: MaterialsOverride) = Unit
        },
    )

    @Test
    fun `follows Remote Config without an override`() = runTest {
        coEvery { remoteConfig.getBoolean("materials_enabled") } returns Result.success(true)

        assertTrue(switch(MaterialsOverride.REMOTE_CONFIG).isOn())
    }

    @Test
    fun `an unreadable Remote Config keeps materials off`() = runTest {
        coEvery { remoteConfig.getBoolean("materials_enabled") } returns Result.failure(IllegalStateException("unset"))

        assertFalse(switch(MaterialsOverride.REMOTE_CONFIG).isOn())
    }

    @Test
    fun `the debug override wins without reading Remote Config`() = runTest {
        coEvery { remoteConfig.getBoolean("materials_enabled") } returns Result.success(true)

        assertTrue(switch(MaterialsOverride.ON).isOn())
        assertFalse(switch(MaterialsOverride.OFF).isOn())
        coVerify(exactly = 0) { remoteConfig.getBoolean(any()) }
    }
}
