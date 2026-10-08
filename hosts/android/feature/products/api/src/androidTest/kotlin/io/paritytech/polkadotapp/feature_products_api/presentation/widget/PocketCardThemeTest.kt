package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.designsystem.colors.LocalPolkadotColors
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class PocketCardThemeTest {
    @get:Rule
    val compose = createComposeRule()

    private val pickedTheme = PolkadotAppTheme.Lisbon

    private val primaryGround = JsWidget.Box(
        modifiers = listOf(
            JsModifier.Size(width = 40, height = 40),
            JsModifier.Background(color = JsColor.FG_PRIMARY),
        ),
    )

    @Before
    fun thePickedThemeDiffersFromTheDefault() {
        assertNotEquals(PolkadotAppTheme.DEFAULT.colors().fg.primary, pickedTheme.colors().fg.primary)
    }

    @Test
    fun aCardFaceLooksTheSameWhateverThemeTheUserPicked() {
        val drawn = drawPrimaryGround { face -> PocketCardTheme(content = face) }

        assertEquals(PolkadotAppTheme.DEFAULT.colors().fg.primary, drawn)
    }

    @Test
    fun aWidgetOutsideAPocketCardFollowsThePickedTheme() {
        val drawn = drawPrimaryGround { face -> face() }

        assertEquals(pickedTheme.colors().fg.primary, drawn)
    }

    private fun drawPrimaryGround(surface: @Composable (face: @Composable () -> Unit) -> Unit): Color {
        compose.setContent {
            CompositionLocalProvider(LocalPolkadotColors provides pickedTheme.colors()) {
                surface {
                    JsWidgetRenderer(
                        widget = primaryGround,
                        modifier = Modifier.testTag(FACE_TAG),
                        jsEventHandler = { _, _ -> },
                    )
                }
            }
        }
        val pixels = compose.onNodeWithTag(FACE_TAG).captureToImage().toPixelMap()
        return pixels[pixels.width / 2, pixels.height / 2]
    }

    private companion object {
        const val FACE_TAG = "face"
    }
}
