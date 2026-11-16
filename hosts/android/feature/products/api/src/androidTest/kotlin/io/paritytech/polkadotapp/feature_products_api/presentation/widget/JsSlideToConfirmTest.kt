package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.compose.ui.test.swipeLeft
import androidx.compose.ui.test.swipeRight
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsSlideToConfirmVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** The slide as a person meets it in a chat card, through the same renderer the chat uses. */
class JsSlideToConfirmTest {
    @get:Rule
    val compose = createComposeRule()

    private val label = "Slide to register"
    private val ready = JsWidget.SlideToConfirm(text = label, onConfirm = "register")
    private val events = mutableListOf<Pair<String, JsUiEvent.Type>>()

    @Test
    fun aSlideToTheEndConfirmsOnceAsAButtonPress() {
        for (variant in JsSlideToConfirmVariant.entries) {
            events.clear()
            draw(ready.copy(variant = variant))

            control().performTouchInput { swipeRight(startX = left + 40f, endX = right, durationMillis = 400) }
            compose.waitForIdle()
            control().performTouchInput { swipeRight(startX = left + 40f, endX = right, durationMillis = 400) }
            compose.waitForIdle()

            assertEquals("$variant", listOf("register" to JsUiEvent.Type.ButtonClick), events)
        }
    }

    @Test
    fun aSlideLetGoHalfwayConfirmsNothing() {
        draw(ready)

        control().performTouchInput { swipeRight(startX = left + 40f, endX = centerX, durationMillis = 400) }
        compose.waitForIdle()

        assertTrue(events.isEmpty())
    }

    // Swipe to reply is a horizontal draggable around the bubble; a drag that starts on the control,
    // in either direction, must not move the bubble.
    @Test
    fun aSlideInsideASwipeToReplyBubbleLeavesTheBubbleStill() {
        var bubbleDragged = false
        draw(ready) { content ->
            Box(
                Modifier
                    .testTag(BUBBLE)
                    .draggable(
                        orientation = Orientation.Horizontal,
                        state = rememberDraggableState { bubbleDragged = true },
                    ),
            ) {
                Column {
                    content()
                    Spacer(Modifier.height(80.dp))
                }
            }
        }

        control().performTouchInput { swipeLeft(startX = right - 10f, endX = left, durationMillis = 400) }
        compose.waitForIdle()
        assertFalse("a leftward drag on the control moved the bubble", bubbleDragged)

        control().performTouchInput { swipeRight(startX = left + 40f, endX = right, durationMillis = 400) }
        compose.waitForIdle()
        assertFalse("the slide moved the bubble", bubbleDragged)
        assertEquals(listOf("register" to JsUiEvent.Type.ButtonClick), events)

        // Below the control, the bubble still swipes.
        compose.onNodeWithTag(BUBBLE).performTouchInput {
            swipe(start = Offset(right - 10f, bottom - 20f), end = Offset(left, bottom - 20f), durationMillis = 400)
        }
        compose.waitForIdle()
        assertTrue("swipe to reply stopped working on the rest of the bubble", bubbleDragged)
    }

    @Test
    fun anUpwardDragScrollsTheFeedAndConfirmsNothing() {
        var scroll: ScrollState? = null
        draw(ready) { content ->
            val state = rememberScrollState()
            scroll = state
            Column(Modifier.height(200.dp).verticalScroll(state)) {
                content()
                Spacer(Modifier.height(1000.dp))
            }
        }

        control().performTouchInput { swipeUp(startY = bottom - 5f, endY = top - 150f, durationMillis = 300) }
        compose.waitForIdle()

        assertTrue("the feed did not scroll", checkNotNull(scroll).value > 0)
        assertTrue(events.isEmpty())
    }

    @Test
    fun aScreenReaderDoubleTapConfirmsOnce() {
        draw(ready)

        control().performSemanticsAction(SemanticsActions.OnClick)
        compose.waitForIdle()

        assertEquals(listOf("register" to JsUiEvent.Type.ButtonClick), events)
        // Slid, the control offers nothing more to activate until the product answers.
        control().assert(SemanticsMatcher.keyNotDefined(SemanticsActions.OnClick))
        control().assertIsNotEnabled()
    }

    @Test
    fun aControlThatAcceptsNoSlideIsReportedDisabledAndIgnoresDrags() {
        val states = mapOf(
            "disabled" to ready.copy(enabled = false),
            "loading" to ready.copy(loading = true),
            "confirmed" to ready.copy(confirmed = true, confirmedText = "Registered"),
            "no action" to ready.copy(onConfirm = null),
        )
        for ((name, widget) in states) {
            events.clear()
            draw(widget)

            val node = compose.onNodeWithContentDescription(if (widget.confirmed) "Registered" else label)
            node.assertIsNotEnabled()
            node.assert(SemanticsMatcher.keyNotDefined(SemanticsActions.OnClick))
            node.performTouchInput { swipeRight(startX = left + 40f, endX = right, durationMillis = 400) }
            compose.waitForIdle()

            assertTrue(name, events.isEmpty())
        }
    }

    @Test
    fun aLoadingControlIsReportedBusy() {
        draw(ready.copy(loading = true))

        control().assert(
            SemanticsMatcher.expectValue(SemanticsProperties.ProgressBarRangeInfo, ProgressBarRangeInfo.Indeterminate),
        )
    }

    @Test
    fun aTreeThatClearsLoadingLetsThePersonSlideAgain() {
        val widget = mutableStateOf(ready)
        compose.setContent {
            Themed { JsWidgetRenderer(widget = widget.value, jsEventHandler = { id, type -> events += id to type }) }
        }

        control().performTouchInput { swipeRight(startX = left + 40f, endX = right, durationMillis = 400) }
        compose.waitForIdle()
        widget.value = ready.copy(loading = true)
        compose.waitForIdle()
        widget.value = ready
        compose.waitForIdle()
        control().performTouchInput { swipeRight(startX = left + 40f, endX = right, durationMillis = 400) }
        compose.waitForIdle()

        assertEquals(2, events.size)
    }

    private fun control() = compose.onNodeWithContentDescription(label, substring = true)

    private val drawn = mutableStateOf<Drawing?>(null)

    private class Drawing(val widget: JsWidget.SlideToConfirm, val around: @Composable (@Composable () -> Unit) -> Unit)

    /** Draws a fresh control; called again, it replaces the control rather than updating it. */
    private fun draw(
        widget: JsWidget.SlideToConfirm,
        around: @Composable (content: @Composable () -> Unit) -> Unit = { it() },
    ) {
        val first = drawn.value == null
        drawn.value = Drawing(widget, around)
        if (first) {
            compose.setContent {
                drawn.value?.let { drawing ->
                    key(drawing) {
                        Themed {
                            drawing.around {
                                Box(Modifier.width(320.dp).padding(8.dp)) {
                                    JsWidgetRenderer(widget = drawing.widget, jsEventHandler = { id, type -> events += id to type })
                                }
                            }
                        }
                    }
                }
            }
        }
        compose.waitForIdle()
    }

    @Composable
    private fun Themed(content: @Composable () -> Unit) {
        PolkadotTheme(theme = PolkadotAppTheme.BerlinNight, content = content)
    }

    private companion object {
        const val BUBBLE = "bubble"
    }
}
