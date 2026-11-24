package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.milliseconds

class GpuBudgetTest {
    private fun verdict(vararg frames: Double): Boolean {
        val budget = GpuBudget()
        return frames.map { budget.record(it.milliseconds) }.last()
    }

    @Test
    fun `a node is over budget when its second and third frames both take more than 30 ms`() {
        assertEquals(
            listOf(true, false, true, false),
            listOf(verdict(50.0, 31.0, 31.0), verdict(50.0, 31.0, 29.0), verdict(10.0, 40.0, 40.0), verdict(50.0, 30.0, 30.0)),
        )
    }
}
