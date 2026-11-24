package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsAnimatedProperty
import io.paritytech.polkadotapp.feature_products_api.model.JsCurve
import io.paritytech.polkadotapp.feature_products_api.model.JsMotion
import io.paritytech.polkadotapp.feature_products_api.model.JsTrack
import io.paritytech.polkadotapp.feature_products_api.model.JsTrackPlay
import io.paritytech.polkadotapp.feature_products_api.model.JsBlendingMode
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class JsBlendTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun screenUnderOpacityBlendsWithTheBackdrop() {
        draw(mutableStateOf(listOf(layer())))

        assertScreen(128f / 255f)
    }

    @Test
    fun animatedOpacityKeepsScreenBlendingWithTheBackdrop() {
        val source = layer()
        val opacity = JsModifier.Animate(JsTrack(JsAnimatedProperty.OPACITY, JsTrackPlay.Follow(500, JsMotion.Timed(1000, JsCurve(0, 0, 1000, 1000)))))
        draw(mutableStateOf(listOf(source.copy(modifiers = source.modifiers + opacity))))
        assertScreen(128f / 255f * 0.5f)
    }

    @Test
    fun screenStillBlendsWithTheBackdropDuringExitFade() {
        compose.mainClock.autoAdvance = false
        val children = mutableStateOf(listOf(layer()))
        draw(children)
        compose.mainClock.advanceTimeByFrame()
        compose.runOnIdle { children.value = emptyList() }
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeByFrame()
        compose.mainClock.advanceTimeBy(500)

        assertScreen(128f / 255f * (1f - WidgetStyleRules.CssEase.transform(0.5f)))
    }

    private fun layer() = JsWidget.Box(
        modifiers = listOf(
            JsModifier.Size(width = 32, height = 32),
            JsModifier.Fill(0x804020FFL),
            JsModifier.Opacity(128),
            JsModifier.BlendingMode(JsBlendingMode.SCREEN),
            JsModifier.ExitFade(1000),
        ),
    )

    private fun draw(children: State<List<JsWidget.Box>>) {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                Box(Modifier.size(32.dp).background(Color(0.2f, 0.4f, 0.6f))) {
                    JsWidgetRenderer(
                        widget = JsWidget.Box(children = children.value),
                        jsEventHandler = { _, _ -> },
                    )
                }
            }
        }
    }

    private fun assertScreen(alpha: Float) {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        val actual = pixels[pixels.width / 2, pixels.height / 2]
        assertEquals(0.2f + (128f / 255f) * 0.8f * alpha, actual.red, 0.025f)
        assertEquals(0.4f + (64f / 255f) * 0.6f * alpha, actual.green, 0.025f)
        assertEquals(0.6f + (32f / 255f) * 0.4f * alpha, actual.blue, 0.025f)
    }
}
