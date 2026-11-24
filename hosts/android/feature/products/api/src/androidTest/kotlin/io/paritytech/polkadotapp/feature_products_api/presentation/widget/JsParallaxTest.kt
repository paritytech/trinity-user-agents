package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsTiltParallax
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** A red node 16 dp wide in a 64 dp black frame, following the drawn offset of a keyed gesture's children. */
@RunWith(AndroidJUnit4::class)
class JsParallaxTest {
    @get:Rule
    val compose = createComposeRule()

    private val position = JsGesturePosition()

    private fun tree(parallax: JsModifier.Parallax) = JsWidget.Box(
        modifiers = listOf(JsModifier.Size(width = 64, height = 32)),
        children = listOf(
            JsWidget.Gesture(
                position = position,
                key = "g",
                modifiers = listOf(JsModifier.Size(width = 64, height = 32)),
                children = listOf(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 8, height = 8)))),
            ),
            JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 16, height = 32), JsModifier.Fill(0xFF0000FF), parallax)),
        ),
    )

    @Test
    fun theNodeFollowsTheChildrenAsTheyAreDrawnScaledByDepth() {
        draw(tree(JsModifier.Parallax(depthPermille = 500, pan = "g")))
        assertRed(at = 8f, value = 1f)
        compose.runOnIdle { position.receive(3200, 0) }
        compose.waitForIdle()
        assertRed(at = 8f, value = 0f)
        assertRed(at = 24f, value = 1f)
    }

    @Test
    fun lessMotionKeepsThePanAndDropsTheTilt() {
        draw(tree(JsModifier.Parallax(depthPermille = 1000, tilt = JsTiltParallax(100_000, 100, 0), pan = "g")), motion = false)
        compose.runOnIdle { position.receive(1600, 0) }
        compose.waitForIdle()
        assertRed(at = 8f, value = 0f)
        assertRed(at = 24f, value = 1f)
    }

    @Test
    fun theMoveFollowsTheLayoutDirection() {
        draw(tree(JsModifier.Parallax(depthPermille = 1000, pan = "g")), direction = LayoutDirection.Rtl)
        compose.runOnIdle { position.receive(1600, 0) }
        compose.waitForIdle()
        assertRed(at = 56f, value = 0f)
        assertRed(at = 40f, value = 1f)
    }

    @Test
    fun aKeyThatNamesNoGestureAddsNothing() {
        draw(tree(JsModifier.Parallax(depthPermille = 1000, pan = "missing")))
        compose.runOnIdle { position.receive(3200, 0) }
        compose.waitForIdle()
        assertRed(at = 8f, value = 1f)
    }

    private fun draw(widget: JsWidget, motion: Boolean = true, direction: LayoutDirection = LayoutDirection.Ltr) {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                CompositionLocalProvider(LocalRendererMotionEnabled provides motion, LocalLayoutDirection provides direction) {
                    Box(Modifier.size(64.dp, 32.dp).background(Color.Black)) {
                        JsWidgetRenderer(widget = widget, jsEventHandler = { _, _ -> })
                    }
                }
            }
        }
    }

    /** The red at [at] dp from the left. */
    private fun assertRed(at: Float, value: Float) {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        assertEquals(value, pixels[(pixels.width * at / 64f).toInt(), pixels.height / 2].red, 0.06f)
    }
}
