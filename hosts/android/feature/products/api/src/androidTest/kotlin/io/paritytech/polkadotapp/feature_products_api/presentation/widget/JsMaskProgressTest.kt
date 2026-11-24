package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskFollow
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptic
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskHaptics
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskPoint
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShape
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanFeel
import io.paritytech.polkadotapp.feature_products_api.model.JsPanGesture
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsShareRescue
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class JsMaskProgressTest {
    @get:Rule
    val compose = createComposeRule()
    private val actions = mutableListOf<String>()

    @Test
    fun shareCanFireBeforeTheFingerLiftsAndDoesNotRepeatOnLift() {
        val trail = JsPanTrail()
        draw(
            JsWidget.Gesture(
                key = "pan", trail = trail,
                modifiers = listOf(JsModifier.Size(width = 193, height = 98)),
                pan = JsPanGesture(axis = JsPanAxis.FREE, location = true),
                children = listOf(
                    JsWidget.Box(
                        modifiers = listOf(
                            JsModifier.Size(width = 193, height = 98), JsModifier.Fill(0xFFFFFFFF),
                            JsModifier.Mask(
                                follow = JsMaskFollow("pan", 4700), trail = trail,
                                clearedShare = JsClearedShare(580, "covered", whileDown = true),
                                haptics = JsMaskHaptics(100, JsMaskHaptic.TICK, JsMaskHaptic.SUCCESS),
                            ),
                        ),
                    ),
                ),
            ),
        )
        strokeDown(25f)
        compose.runOnIdle { assertEquals(emptyList<String>(), actions) }
        lift()
        strokeDown(75f)
        compose.runOnIdle { assertEquals(listOf("covered"), actions) }
        lift()
        compose.runOnIdle { assertEquals(listOf("covered"), actions) }
    }

    @Test
    fun stationaryCoverageCanBeRescuedWithoutAnotherTouch() {
        draw(
            JsWidget.Box(
                modifiers = listOf(
                    JsModifier.Size(width = 10, height = 10), JsModifier.Fill(0xFFFFFFFF),
                    JsModifier.Mask(
                        shapes = listOf(JsMaskShape.Circle(JsMaskPoint(500, 500), 500)),
                        clearedShare = JsClearedShare(900, "rescued", rescue = JsShareRescue(100, 500)),
                    ),
                ),
            ),
        )
        compose.waitUntil(5000) {
            compose.mainClock.advanceTimeByFrame()
            actions.isNotEmpty()
        }
        compose.mainClock.advanceTimeBy(1000)
        compose.runOnIdle { assertEquals(listOf("rescued"), actions) }
    }

    @Test
    fun pressingCanFlattenChildrenWithoutChangingTheirWidth() {
        draw(
            JsWidget.Gesture(
                modifiers = listOf(JsModifier.Size(width = 100, height = 100)),
                pan = JsPanGesture(axis = JsPanAxis.FREE, location = true),
                feel = JsPanFeel(pressScale = 1f, pressScaleY = 0.5f),
                children = listOf(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 100, height = 100), JsModifier.Fill(0xFFFFFFFF)))),
            ),
        )
        compose.onRoot().performTouchInput { down(center) }
        compose.mainClock.advanceTimeBy(48)
        val pressed = compose.onRoot().captureToImage().toPixelMap()
        assertEquals(0f, pressed[pressed.width / 2, pressed.height / 10].red, 0.05f)
        assertEquals(1f, pressed[pressed.width / 10, pressed.height / 2].red, 0.05f)
        lift()
        val released = compose.onRoot().captureToImage().toPixelMap()
        assertEquals(1f, released[released.width / 2, released.height / 10].red, 0.05f)
    }

    @Test
    fun aFollowingMaskStaysUnderTheFingerDuringVerticalPressScaling() = checkFollowingPress(stationary = false)

    @Test
    fun aStationaryFingerStaysAlignedWhileThePressAnimates() = checkFollowingPress(stationary = true)

    private fun checkFollowingPress(stationary: Boolean) {
        val trail = JsPanTrail()
        draw(
            JsWidget.Gesture(
                key = "pan", trail = trail,
                modifiers = listOf(JsModifier.Size(width = 100, height = 100)),
                pan = JsPanGesture(axis = JsPanAxis.FREE, location = true),
                feel = JsPanFeel(pressScale = 1f, pressScaleY = 0.5f, pressMs = 96),
                children = listOf(
                    JsWidget.Box(
                        modifiers = listOf(
                            JsModifier.Size(width = 100, height = 100), JsModifier.Fill(0xFFFFFFFF),
                            JsModifier.Mask(follow = JsMaskFollow("pan", 400), trail = trail),
                        ),
                    ),
                ),
            ),
        )
        compose.onRoot().performTouchInput {
            down(if (stationary) Offset(width * 0.5f, height * 0.4f) else center)
        }
        compose.mainClock.advanceTimeBy(160)
        if (!stationary) compose.onRoot().performTouchInput { moveTo(Offset(width * 0.5f, height * 0.4f)) }
        compose.mainClock.advanceTimeBy(48)
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        assertEquals("the mask clears at the drawn finger position", 0f, pixels[pixels.width / 2, (pixels.height * 0.4f).toInt()].red, 0.05f)
        lift()
    }

    private fun draw(widget: JsWidget) {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                val view = LocalView.current
                SideEffect { view.isHapticFeedbackEnabled = false }
                Box(Modifier.background(Color.Black)) {
                    JsWidgetRenderer(widget = widget, jsEventHandler = { action, _ -> actions += action })
                }
            }
        }
        compose.mainClock.autoAdvance = false
        compose.mainClock.advanceTimeBy(48)
    }

    private fun strokeDown(y: Float) {
        compose.onRoot().performTouchInput {
            down(Offset(width * 15f / 193f, height * y / 98f))
            moveTo(Offset(width * 175f / 193f, height * y / 98f), delayMillis = 600)
        }
        compose.mainClock.advanceTimeBy(48)
    }

    private fun lift() {
        compose.onRoot().performTouchInput { up() }
        compose.mainClock.advanceTimeBy(48)
    }
}
