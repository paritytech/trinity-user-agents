package io.paritytech.polkadotapp.common.data.platform

import android.content.Context
import android.os.BatteryManager
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import javax.inject.Inject
import javax.inject.Singleton

interface BatteryService {
    fun currentBatteryLevel(): Fraction
}

@Singleton
internal class RealBatteryService @Inject constructor(
    @ApplicationContext context: Context
) : BatteryService {
    private val batteryManager = context.getSystemService(Context.BATTERY_SERVICE) as BatteryManager

    override fun currentBatteryLevel(): Fraction {
        return batteryManager.getIntProperty(BatteryManager.BATTERY_PROPERTY_CAPACITY).percents
    }
}
