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
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsBurst
import io.paritytech.polkadotapp.feature_products_api.model.JsBurstTrigger
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitSource
import io.paritytech.polkadotapp.feature_products_api.model.JsEmitter
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsParticleLook
import io.paritytech.polkadotapp.feature_products_api.model.JsValueRange
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Particles at rest (no speed, no spread), so where they are drawn does not depend on the random picks. */
@RunWith(AndroidJUnit4::class)
class JsParticlesTest {
    @get:Rule
    val compose = createComposeRule()

    private fun emitter(trigger: JsBurstTrigger, x: Int = 1600) = JsEmitter(
        looks = listOf(JsParticleLook.Circle(800, 0xFF0000FF)),
        source = JsEmitSource.Point(x, 1600),
        ratePerS = 0,
        ratePer100pt = 0,
        lifeMs = 1000,
        speed = JsValueRange(0, 0),
        angleCentideg = 0,
        spreadCentideg = 0,
        gravity = 0,
        spin = JsValueRange(0, 0),
        fadeFromPermille = 1000,
        endScalePermille = 1000,
        maxAlive = 16,
        bursts = listOf(JsBurst(1, trigger)),
    )

    private fun node(emitter: JsEmitter?) =
        JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 32, height = 32)) + listOfNotNull(emitter?.let { JsModifier.Emit(it) }))

    private fun parent(vararg children: JsWidget) = JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 64, height = 32)), children = children.toList())

    @Test
    fun anAppearingNodeBurstsOverItselfUntilTheParticlesLifeEnds() {
        draw(mutableStateOf(parent(node(emitter(JsBurstTrigger.Appear)))))
        frames()
        assertRed(at = 0.25f, value = 1f)
        assertRed(at = 0.125f, value = 0f)
        compose.mainClock.advanceTimeBy(1100)
        assertRed(at = 0.25f, value = 0f)
    }

    @Test
    fun lessMotionSendsNoParticles() {
        draw(mutableStateOf(parent(node(emitter(JsBurstTrigger.Appear)))), motion = false)
        frames()
        assertRed(at = 0.25f, value = 0f)
    }

    @Test
    fun aPointCountsFromTheStartEdgeRightToLeft() {
        draw(mutableStateOf(parent(node(emitter(JsBurstTrigger.Appear, x = 800)))), direction = LayoutDirection.Rtl)
        frames()
        // The node sits at the right half; 8 dp from its start edge is 56 dp from the left.
        assertRed(at = 56f / 64f, value = 1f)
        assertRed(at = 40f / 64f, value = 0f)
    }

    @Test
    fun particlesOutliveANodeTheTreeDrops() {
        val widget = mutableStateOf(parent(node(emitter(JsBurstTrigger.Appear))))
        draw(widget)
        frames()
        compose.runOnIdle { widget.value = parent() }
        frames()
        assertRed(at = 0.25f, value = 1f)
        compose.mainClock.advanceTimeBy(1100)
        assertRed(at = 0.25f, value = 0f)
    }

    @Test
    fun particlesOutliveAnEmitterTheTreeDrops() {
        val widget = mutableStateOf(parent(node(emitter(JsBurstTrigger.Appear))))
        draw(widget)
        frames()
        compose.runOnIdle { widget.value = parent(node(null)) }
        frames()
        assertRed(at = 0.25f, value = 1f)
    }

    @Test
    fun aChangedValueBurstsAndTheFirstTreeDoesNot() {
        val widget = mutableStateOf(parent(node(emitter(JsBurstTrigger.Change(1)))))
        draw(widget)
        frames()
        assertRed(at = 0.25f, value = 0f)
        compose.runOnIdle { widget.value = parent(node(emitter(JsBurstTrigger.Change(1)))) }
        frames()
        assertRed(at = 0.25f, value = 0f)
        compose.runOnIdle { widget.value = parent(node(emitter(JsBurstTrigger.Change(2)))) }
        frames()
        assertRed(at = 0.25f, value = 1f)
    }

    @Test
    fun anActionFromAnotherNodeBurstsAsItIsSent() {
        val sent = mutableListOf<String>()
        val tap = JsWidget.Gesture(tapAction = "go", accessibilityLabel = "tap", modifiers = listOf(JsModifier.Size(width = 32, height = 32)))
        draw(mutableStateOf(parent(node(emitter(JsBurstTrigger.Action("go"))), tap)), sent = sent)
        frames()
        assertRed(at = 0.25f, value = 0f)
        compose.onNodeWithContentDescription("tap").performSemanticsAction(SemanticsActions.OnClick)
        frames()
        assertEquals(listOf("go"), sent)
        assertRed(at = 0.25f, value = 1f)
    }

    private fun frames() = repeat(3) { compose.mainClock.advanceTimeByFrame() }

    private fun draw(
        widget: State<JsWidget>,
        motion: Boolean = true,
        direction: LayoutDirection = LayoutDirection.Ltr,
        sent: MutableList<String> = mutableListOf(),
    ) {
        compose.mainClock.autoAdvance = false
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                CompositionLocalProvider(LocalRendererMotionEnabled provides motion, LocalLayoutDirection provides direction) {
                    Box(Modifier.size(64.dp, 32.dp).background(Color.Black)) {
                        JsWidgetRenderer(widget = widget.value, jsEventHandler = { action, _ -> sent += action })
                    }
                }
            }
        }
    }

    private fun assertRed(at: Float, value: Float) {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        assertEquals(value, pixels[(pixels.width * at).toInt(), pixels.height / 2].red, 0.06f)
    }
}
