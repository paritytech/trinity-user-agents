package io.paritytech.polkadotapp.feature_products_impl.data.config

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class RemoteConfigMaterialsSwitchTest {
    private fun switchReading(value: Result<Boolean>) = RemoteConfigMaterialsSwitch(
        mockk<RemoteConfigService> { coEvery { getBoolean("materials_enabled") } returns value },
    )

    @Test
    fun `materials are on only when the key reads true`() = runTest {
        assertEquals(true, switchReading(Result.success(true)).isOn())
        assertEquals(false, switchReading(Result.success(false)).isOn())
    }

    // A switch that cannot be read must not turn on code the team may have switched off.
    @Test
    fun `an unreadable key reads off`() = runTest {
        assertEquals(false, switchReading(Result.failure(IllegalStateException("not activated"))).isOn())
    }
}
