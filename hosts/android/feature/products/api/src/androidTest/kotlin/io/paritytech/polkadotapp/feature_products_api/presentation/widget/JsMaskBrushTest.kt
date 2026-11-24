package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.LayoutDirection
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrush
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskBrushTip
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class JsMaskBrushTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun ellipseDrawsAtItsFixedAngle() = checkTip(JsMaskBrushTip.ELLIPSE, LayoutDirection.Ltr)

    @Test
    fun capsuleDrawsAtItsFixedAngle() = checkTip(JsMaskBrushTip.CAPSULE, LayoutDirection.Ltr)

    @Test
    fun ellipseMirrorsItsFixedAngleInRightToLeftLayout() = checkTip(JsMaskBrushTip.ELLIPSE, LayoutDirection.Rtl)

    @Test
    fun capsuleMirrorsItsFixedAngleInRightToLeftLayout() = checkTip(JsMaskBrushTip.CAPSULE, LayoutDirection.Rtl)

    private fun checkTip(tip: JsMaskBrushTip, direction: LayoutDirection) {
        val brush = JsMaskBrush(tip, 200, 4500, 500)
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                CompositionLocalProvider(LocalLayoutDirection provides direction) {
                    Box(Modifier.background(Color.Black)) {
                        JsWidgetRenderer(
                            widget = JsWidget.Box(
                                modifiers = listOf(
                                    JsModifier.Size(width = 20, height = 20),
                                    JsModifier.Fill(0xFFFFFFFF),
                                    JsModifier.Mask(shapes = listOf(JsMaskShape.Stamped(1400, brush, listOf(JsMaskPoint(1000, 1000))))),
                                ),
                            ),
                            jsEventHandler = { _, _ -> },
                        )
                    }
                }
            }
        }
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        fun red(x: Float, y: Float) = pixels[(x / 20 * pixels.width).toInt(), (y / 20 * pixels.height).toInt()].red
        val mirrored = direction == LayoutDirection.Rtl
        assertEquals(0f, red(7f, if (mirrored) 13f else 7f), 0.05f)
        assertEquals(0f, red(13f, if (mirrored) 7f else 13f), 0.05f)
        assertEquals(1f, red(7f, if (mirrored) 7f else 13f), 0.05f)
    }
}
