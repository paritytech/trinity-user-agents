package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskFollow
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanGesture
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class JsMaskTrailReplacementTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun `strokes cover the mask when a trail is replaced without moving its node`() {
        val trail = mutableStateOf<JsPanTrail?>(JsPanTrail())
        val channel = JsMaskShapes()
        compose.setContent {
            Box(Modifier.padding(start = 40.dp, top = 200.dp)) {
                val current = trail.value
                JsGesture(
                    modifier = Modifier.size(193.dp, 98.dp).testTag("pan"),
                    widget = JsWidget.Gesture(
                        key = "pan",
                        trail = current,
                        pan = current?.let { JsPanGesture(axis = JsPanAxis.FREE, location = true) },
                    ),
                    onAction = { _, _ -> },
                ) {
                    val mask = JsModifier.Mask(
                        key = "surface",
                        trail = current,
                        follow = current?.let { JsMaskFollow("pan", 4700) },
                        channel = channel,
                    )
                    Box(Modifier.fillMaxSize().then(mask.maskModifier()))
                }
            }
        }
        compose.waitForIdle()
        compose.runOnIdle { trail.value = null }
        compose.waitForIdle()
        val fresh = JsPanTrail()
        compose.runOnIdle { trail.value = fresh }
        compose.waitForIdle()

        for (row in listOf(18f, 50f, 82f)) {
            compose.onNodeWithTag("pan").performTouchInput {
                swipe(Offset(width * 10f / 193f, height * row / 98f), Offset(width * 183f / 193f, height * row / 98f), 900)
            }
        }

        compose.runOnIdle {
            assertTrue("the pan recorded its strokes", fresh.strokes.isNotEmpty())
            val shapes = requireNotNull(channel.source).invoke().shapes
            val (covered, total) = MaskCoverage.count(shapes, 19300, 9800)
            assertTrue(
                "covered $covered/$total, trail origin ${fresh.origin}, coordinates ${fresh.coordinates}",
                covered * 1000 >= total * 580,
            )
        }
    }
}
