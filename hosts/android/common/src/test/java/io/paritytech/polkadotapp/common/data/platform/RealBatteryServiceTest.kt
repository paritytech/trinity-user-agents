package io.paritytech.polkadotapp.common.data.platform

import android.content.Context
import android.os.BatteryManager
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RealBatteryServiceTest {
    @Test
    fun `reports no level when the device does not support battery capacity`() {
        assertNull(batteryLevelFor(capacity = Int.MIN_VALUE))
    }

    @Test
    fun `reports the capacity as a percentage across the whole range`() {
        assertEquals(
            listOf(0.percents, 37.percents, 100.percents),
            listOf(0, 37, 100).map { batteryLevelFor(capacity = it) }
        )
    }

    private fun batteryLevelFor(capacity: Int): Fraction? {
        val batteryManager = mockk<BatteryManager> {
            every { getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY) } returns capacity
        }
        val context = mockk<Context> {
            every { getSystemService(Context.BATTERY_SERVICE) } returns batteryManager
        }

        return RealBatteryService(context).currentBatteryLevel()
    }
}
