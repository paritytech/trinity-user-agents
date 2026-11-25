package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.TouchInjectionScope
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePhase
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanDrag
import io.paritytech.polkadotapp.feature_products_api.model.JsPanGesture
import io.paritytech.polkadotapp.feature_products_api.model.JsPanRelease
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** A touch the system takes away (a cancel) ends a pan as Cancelled, never as a lift, as on iOS. */
@RunWith(AndroidJUnit4::class)
class JsGestureCancelTest {
    @get:Rule
    val compose = createComposeRule()
    private val ends = mutableListOf<JsGesturePhase>()
    private val sent = mutableListOf<Pair<String, JsGesturePhase>>()

    @Test
    fun aCancelledDragPastTheThresholdIsNotACompletion() {
        drawDrag(JsPanRelease.Complete(80))
        dragAcross { cancel() }
        compose.runOnIdle { assertEquals(listOf(JsGesturePhase.CANCELLED), ends) }
    }

    @Test
    fun aLiftedDragPastTheThresholdStillCompletes() {
        drawDrag(JsPanRelease.Complete(80))
        dragAcross { up() }
        compose.runOnIdle { assertEquals(listOf(JsGesturePhase.COMPLETED), ends) }
    }

    @Test
    fun aCancelledSnapBackDragIsCancelled() {
        drawDrag(JsPanRelease.SnapBack)
        dragAcross { cancel() }
        compose.runOnIdle { assertEquals(listOf(JsGesturePhase.CANCELLED), ends) }
    }

    @Test
    fun aCancelledStayDragIsCancelled() {
        drawDrag(JsPanRelease.Stay)
        dragAcross { cancel() }
        compose.runOnIdle { assertEquals(listOf(JsGesturePhase.CANCELLED), ends) }
    }

    @Test
    fun aCancelledLocationPanIsCancelled() {
        draw(
            JsWidget.Gesture(
                modifiers = listOf(JsModifier.Size(width = 300, height = 100)),
                pan = JsPanGesture(axis = JsPanAxis.FREE, location = true, endAction = END),
                children = listOf(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 300, height = 100)))),
            ),
        )
        dragAcross { cancel() }
        compose.runOnIdle { assertEquals(listOf(JsGesturePhase.CANCELLED), ends) }
    }

    @Test
    fun aLiftedLocationPanEnds() {
        draw(
            JsWidget.Gesture(
                modifiers = listOf(JsModifier.Size(width = 300, height = 100)),
                pan = JsPanGesture(axis = JsPanAxis.FREE, location = true, endAction = END),
                children = listOf(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 300, height = 100)))),
            ),
        )
        dragAcross { up() }
        compose.runOnIdle { assertEquals(listOf(JsGesturePhase.ENDED), ends) }
    }

    @Test
    fun aLiftAPanInsideTookStillCompletesTheDragAround() {
        draw(
            JsWidget.Gesture(
                modifiers = listOf(JsModifier.Size(width = 300, height = 100)),
                pan = JsPanGesture(drag = JsPanDrag(release = JsPanRelease.Complete(80)), endAction = END),
                children = listOf(
                    JsWidget.Gesture(
                        modifiers = listOf(JsModifier.Size(width = 50, height = 100)),
                        pan = JsPanGesture(axis = JsPanAxis.FREE, location = true, endAction = INNER_END),
                        children = listOf(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 50, height = 100)))),
                    ),
                ),
            ),
        )
        dragAcross { up() }
        compose.runOnIdle {
            assertEquals(listOf(INNER_END to JsGesturePhase.ENDED, END to JsGesturePhase.COMPLETED), sent)
        }
    }

    private fun drawDrag(release: JsPanRelease) = draw(
        JsWidget.Gesture(
            modifiers = listOf(JsModifier.Size(width = 300, height = 100)),
            pan = JsPanGesture(drag = JsPanDrag(release = release), endAction = END),
            children = listOf(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 50, height = 100)))),
        ),
    )

    /** Down on the child at the start edge, along to the far end in steps, then [end]s the touch. */
    private fun dragAcross(end: TouchInjectionScope.() -> Unit) {
        compose.onRoot().performTouchInput {
            down(Offset(width * 25f / 300f, height / 2f))
            repeat(10) { moveBy(Offset(width * 27f / 300f, 0f), delayMillis = 16) }
        }
        compose.mainClock.advanceTimeBy(48)
        compose.onRoot().performTouchInput(end)
        compose.mainClock.advanceTimeBy(48)
    }

    private fun draw(widget: JsWidget) {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                val view = LocalView.current
                SideEffect { view.isHapticFeedbackEnabled = false }
                Box(Modifier.background(Color.Black)) {
                    JsWidgetRenderer(widget = widget, jsEventHandler = { action, event ->
                        val phase = (event as? JsUiEvent.Type.Gesture)?.phase
                        if (action == END && phase != null) ends += phase
                        if (phase != null) sent += action to phase
                    })
                }
            }
        }
        compose.mainClock.autoAdvance = false
        compose.mainClock.advanceTimeBy(48)
    }

    private companion object {
        const val END = "end"
        const val INNER_END = "innerEnd"
    }
}
