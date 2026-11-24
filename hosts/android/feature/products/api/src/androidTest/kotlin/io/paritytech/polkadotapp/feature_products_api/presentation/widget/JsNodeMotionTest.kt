package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsAnimatedProperty
import io.paritytech.polkadotapp.feature_products_api.model.JsCurve
import io.paritytech.polkadotapp.feature_products_api.model.JsEnter
import io.paritytech.polkadotapp.feature_products_api.model.JsKeyframe
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsMotion
import io.paritytech.polkadotapp.feature_products_api.model.JsPose
import io.paritytech.polkadotapp.feature_products_api.model.JsTrack
import io.paritytech.polkadotapp.feature_products_api.model.JsTrackPlay
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class JsNodeMotionTest {
    @get:Rule
    val compose = createComposeRule()
    private val linear = JsCurve(0, 0, 1000, 1000)
    private val timed = JsMotion.Timed(1000, linear)

    @Test
    fun enterAndTimelineMultiplyAndKeepTheirClockAcrossATree() {
        val enter = JsModifier.Enter(JsEnter(JsPose(0, 1000, 0, 0, 0), timed, 0))
        val track = JsModifier.Animate(JsTrack(JsAnimatedProperty.OPACITY, JsTrackPlay.Keyframes(
            listOf(JsKeyframe(0, 0, linear), JsKeyframe(1000, 1000, linear)), 1, false,
        )))
        val widget = mutableStateOf(box(listOf(enter, track)))
        draw(widget)
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(500)
        assertRed(0.25f)
        compose.runOnIdle { widget.value = box(listOf(enter, track, JsModifier.Opacity(255))) }
        compose.mainClock.advanceTimeBy(250)
        assertRed(0.5625f)
    }

    @Test
    fun aFollowingTargetStartsAtItsValueAndAnimatesALaterTree() {
        fun target(value: Int) = box(listOf(JsModifier.Animate(JsTrack(JsAnimatedProperty.OPACITY, JsTrackPlay.Follow(value, timed)))))
        val widget = mutableStateOf(target(1000))
        draw(widget)
        compose.mainClock.advanceTimeByFrame()
        assertRed(1f)
        compose.runOnIdle { widget.value = target(0) }
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(500)
        assertRed(0.5f)
    }

    @Test
    fun removingATrackRestoresThePropertyOnTheNextTree() {
        val widget = mutableStateOf(box(listOf(JsModifier.Animate(JsTrack(JsAnimatedProperty.OPACITY, JsTrackPlay.Follow(250, timed))))))
        draw(widget)
        compose.mainClock.advanceTimeByFrame()
        assertRed(0.25f)
        compose.runOnIdle { widget.value = box(emptyList()) }
        compose.mainClock.advanceTimeByFrame()
        assertRed(1f)
    }

    @Test
    fun lessMotionSkipsEnterAndUsesTheFirstFrameOfAnInfiniteTimeline() {
        val enter = JsModifier.Enter(JsEnter(JsPose(0, 0, 10000, 10000, 9000), timed, 100))
        val track = JsModifier.Animate(JsTrack(JsAnimatedProperty.OPACITY, JsTrackPlay.Keyframes(
            listOf(JsKeyframe(0, 500, linear), JsKeyframe(1000, 1000, linear)), 0, false,
        )))
        draw(mutableStateOf(box(listOf(enter, track))), motion = false)
        assertRed(0.5f)
    }

    @Test
    fun animatedOffsetsAddToTheNodesTranslationWithoutChangingLayout() {
        val widget = box(listOf(
            JsModifier.Translate(8f, 0f),
            JsModifier.Animate(JsTrack(JsAnimatedProperty.OFFSET_X, JsTrackPlay.Follow(800, timed))),
        ))
        draw(mutableStateOf(widget))
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        assertEquals(0f, pixels[pixels.width / 8, pixels.height / 2].red, 0.05f)
        assertEquals(1f, pixels[pixels.width / 2, pixels.height / 2].red, 0.05f)
    }

    @Test
    fun animatedOffsetsFollowTheEndDirectionInRightToLeftLayout() {
        val widget = box(listOf(
            JsModifier.Translate(8f, 0f),
            JsModifier.Animate(JsTrack(JsAnimatedProperty.OFFSET_X, JsTrackPlay.Follow(800, timed))),
        ))
        draw(mutableStateOf(widget), direction = LayoutDirection.Rtl)
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        assertEquals(1f, pixels[pixels.width * 3 / 8, pixels.height / 2].red, 0.05f)
        assertEquals(0f, pixels[pixels.width * 7 / 8, pixels.height / 2].red, 0.05f)
    }

    private fun box(motion: List<JsModifier>) = JsWidget.Box(
        modifiers = listOf(JsModifier.Size(width = 32, height = 32), JsModifier.Fill(0xFFFFFFFF)) + motion,
    )

    private fun draw(widget: State<JsWidget.Box>, motion: Boolean = true, direction: LayoutDirection = LayoutDirection.Ltr) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                CompositionLocalProvider(LocalRendererMotionEnabled provides motion, LocalLayoutDirection provides direction) {
                    Box(Modifier.size(64.dp, 32.dp).background(Color.Black)) {
                        JsWidgetRenderer(widget = widget.value, jsEventHandler = { _, _ -> })
                    }
                }
            }
        }
    }

    private fun assertRed(value: Float) {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        assertEquals(value, pixels[pixels.width / 4, pixels.height / 2].red, 0.06f)
    }
}
