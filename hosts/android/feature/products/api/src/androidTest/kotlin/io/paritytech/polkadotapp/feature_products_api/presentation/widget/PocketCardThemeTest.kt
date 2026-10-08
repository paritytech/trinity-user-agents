package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PocketCardThemeTest {
    @get:Rule
    val compose = createComposeRule()

    private var pickedTheme by mutableStateOf(PolkadotAppTheme.Lisbon)

    private val primaryGround = JsWidget.Box(
        modifiers = listOf(
            JsModifier.Size(width = 40, height = 40),
            JsModifier.Background(color = JsColor.FG_PRIMARY),
        ),
    )

    private val faceUsingThemeDefaults = JsWidget.Column(
        children = listOf(
            JsWidget.Text(text = "Loyalty"),
            JsWidget.Button(text = "Remove", variant = JsButtonVariant.TEXT),
            JsWidget.Button(text = "Open", variant = JsButtonVariant.SECONDARY),
            primaryGround,
        ),
    )

    @Test
    fun aCardFaceIsDrawnInTheDefaultTheme() {
        draw(primaryGround) { face -> PocketCardTheme(content = face) }

        assertEquals(PolkadotAppTheme.DEFAULT.colors().fg.primary, captureFace().centre())
    }

    @Test
    fun aCardFaceLooksTheSameWhateverThemeTheUserPicked() {
        draw(faceUsingThemeDefaults) { face -> PocketCardTheme(content = face) }

        assertTrue(drawnTheSameUnder(PolkadotAppTheme.Lisbon, PolkadotAppTheme.BerlinDay))
    }

    @Test
    fun aWidgetOutsideAPocketCardFollowsThePickedTheme() {
        draw(faceUsingThemeDefaults) { face -> face() }

        assertFalse(drawnTheSameUnder(PolkadotAppTheme.Lisbon, PolkadotAppTheme.BerlinDay))
    }

    private fun draw(widget: JsWidget, surface: @Composable (face: @Composable () -> Unit) -> Unit) {
        compose.setContent {
            PolkadotTheme(theme = pickedTheme) {
                surface {
                    JsWidgetRenderer(widget = widget, modifier = Modifier.testTag(FACE_TAG), jsEventHandler = { _, _ -> })
                }
            }
        }
    }

    private fun drawnTheSameUnder(first: PolkadotAppTheme, second: PolkadotAppTheme): Boolean {
        pickedTheme = first
        val underFirst = captureFace().pixels()
        pickedTheme = second
        val underSecond = captureFace().pixels()
        return underFirst.contentEquals(underSecond)
    }

    private fun captureFace(): ImageBitmap = compose.onNodeWithTag(FACE_TAG).captureToImage()

    private fun ImageBitmap.pixels(): IntArray = IntArray(width * height).also { readPixels(it) }

    private fun ImageBitmap.centre(): Color = toPixelMap()[width / 2, height / 2]

    private companion object {
        const val FACE_TAG = "face"
    }
}
