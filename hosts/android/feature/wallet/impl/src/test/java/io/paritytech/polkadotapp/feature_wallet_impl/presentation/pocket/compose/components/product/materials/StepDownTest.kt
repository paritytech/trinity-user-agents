package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

class StepDownTest {
    // A phone hovering at the limit must not switch materials on and off.
    @Test
    fun strainDrawsPlainAndOnlyACalmPeriodRestores() {
        val stepDown = StepDown()

        assertEquals(true, stepDown.update(now = 0.milliseconds, strained = true))
        assertEquals(true, stepDown.update(now = 1_000.milliseconds, strained = false))
        assertEquals(true, stepDown.update(now = 30_999.milliseconds, strained = false))
        assertEquals(false, stepDown.update(now = 31_000.milliseconds, strained = false))
    }
}
