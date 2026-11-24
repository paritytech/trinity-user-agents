package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.ui.geometry.Offset
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PanPressTest {
    @Test
    fun `a finger on the drawn children, or within the reach of them, presses them`() {
        assertTrue(PanPress.reaches(Offset(60f, 28f), start = 40f, top = 0f, width = 56f, height = 56f, reachPx = 10f))
        assertTrue(PanPress.reaches(Offset(30f, 28f), start = 40f, top = 0f, width = 56f, height = 56f, reachPx = 10f))
        assertTrue(PanPress.reaches(Offset(106f, 66f), start = 40f, top = 0f, width = 56f, height = 56f, reachPx = 10f))
    }

    @Test
    fun `a finger further away does not press them`() {
        assertFalse(PanPress.reaches(Offset(29f, 28f), start = 40f, top = 0f, width = 56f, height = 56f, reachPx = 10f))
        assertFalse(PanPress.reaches(Offset(60f, 67f), start = 40f, top = 0f, width = 56f, height = 56f, reachPx = 10f))
    }
}
