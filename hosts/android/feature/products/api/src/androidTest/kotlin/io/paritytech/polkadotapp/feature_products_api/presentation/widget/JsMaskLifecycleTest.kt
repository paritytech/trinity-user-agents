package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsClearedShare
import io.paritytech.polkadotapp.feature_products_api.model.JsGesturePosition
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskFollow
import io.paritytech.polkadotapp.feature_products_api.model.JsMaskShapes
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsPanAxis
import io.paritytech.polkadotapp.feature_products_api.model.JsPanGesture
import io.paritytech.polkadotapp.feature_products_api.model.JsPanTrail
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class JsMaskLifecycleTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aFollowingMaskReachesItsShareAfterATreeRemovesAFadingSibling() {
        val fixture = Fixture()
        draw(fixture)

        stroke(25f)
        stroke(75f)

        compose.runOnIdle { assertEquals(listOf("covered"), fixture.actions) }
    }

    private fun draw(fixture: Fixture) {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                Box(Modifier.background(Color.Black)) {
                    JsWidgetRenderer(widget = fixture.tree.value, jsEventHandler = { action, _ -> fixture.receive(action) })
                }
            }
        }
    }

    private fun stroke(y: Float) {
        compose.onRoot().performTouchInput {
            swipe(
                start = Offset(width * 15f / 193f, height * y / 98f),
                end = Offset(width * 175f / 193f, height * y / 98f),
                durationMillis = 600,
            )
        }
        compose.waitForIdle()
    }

    private class Fixture {
        val position = JsGesturePosition()
        val trail = JsPanTrail()
        val channel = JsMaskShapes()
        val actions = mutableListOf<String>()
        var lifted = false
        var mask = JsModifier.Mask(
            key = "surface",
            follow = JsMaskFollow("pan", 4700),
            clearedShare = JsClearedShare(580, "covered"),
            channel = channel,
            trail = trail,
        )
        val tree = mutableStateOf(widget(showHint = true))

        fun receive(action: String) {
            if (action == "covered") actions.add(action)
            if (action == "end" && !lifted) {
                lifted = true
                mask = mask.copy(shapes = requireNotNull(channel.source).invoke().shapes)
                channel.treeArrived(mask)
                trail.reset()
                position.receive(0, 0)
                tree.value = widget(showHint = false)
            } else {
                channel.extend(emptyList())
                position.acknowledge()
            }
        }

        private fun widget(showHint: Boolean): JsWidget = JsWidget.Gesture(
            key = "pan",
            position = position,
            trail = trail,
            modifiers = listOf(JsModifier.Size(width = 193, height = 98)),
            pan = JsPanGesture(
                axis = JsPanAxis.FREE,
                location = true,
                startAction = "start",
                moveAction = "move",
                endAction = "end",
            ),
            children = buildList {
                add(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 193, height = 98), JsModifier.Fill(0xBBBBBBFF), mask)))
                if (showHint) {
                    add(
                        JsWidget.Box(
                            modifiers = buildList {
                                add(JsModifier.Size(width = 193, height = 98))
                                add(JsModifier.ExitFade(250))
                            },
                            children = listOf(JsWidget.Box(modifiers = listOf(JsModifier.Size(width = 12, height = 12), JsModifier.Fill(0xFFFFFFFF)))),
                        ),
                    )
                }
            },
        )
    }
}
