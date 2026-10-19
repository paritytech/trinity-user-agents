package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
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
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsImageFit
import io.paritytech.polkadotapp.feature_products_api.model.JsImageSource
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import kotlin.math.abs

/** The layout rules of `renderer.rs`, which iOS draws the same way. */
@RunWith(AndroidJUnit4::class)
class JsWidgetLayoutTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun aSpacerWithNoSizeTakesWhatItsRowHasLeft() {
        draw(
            JsWidget.Row(
                modifiers = listOf(JsModifier.Size(width = 100)),
                children = listOf(block(JsColor.FG_ERROR), JsWidget.Spacer(), block(JsColor.FG_SUCCESS)),
            ),
        )

        assertEquals(80, boundsOf(success)?.left?.inDp())
    }

    @Test
    fun aChildFillingTheHeightOfARowWithNoHeightIsAsTallAsTheTallestChild() {
        draw(
            JsWidget.Row(
                children = listOf(
                    block(JsColor.FG_ERROR),
                    JsWidget.Box(
                        modifiers = listOf(
                            JsModifier.Size(width = 20),
                            JsModifier.FillMaxHeight(),
                            JsModifier.Background(JsColor.FG_SUCCESS),
                        ),
                    ),
                ),
            ),
        )

        assertEquals(20, boundsOf(success)?.height()?.inDp())
    }

    @Test
    fun aPictureWithNoSizeOfItsOwnTakesOneDpPerPixel() {
        val image = JsWidget.Image(source = JsImageSource.Archive("small.png"), fit = JsImageFit.CONTAIN)

        val drawn = drawPicture(30, 20, image)

        assertEquals(listOf(30, 20), listOf(drawn.width().inDp(), drawn.height().inDp()))
    }

    @Test
    fun aLargePictureWithNoSizeOfItsOwnIsShrunkToFit() {
        val image = JsWidget.Image(source = JsImageSource.Archive("large.png"), fit = JsImageFit.FILL)

        val drawn = drawPicture(600, 300, image)

        assertEquals(listOf(100, 50), listOf(drawn.width().inDp(), drawn.height().inDp()))
    }

    @Test
    fun aPictureSizedOnOneSideTakesTheOtherFromItsProportions() {
        val image = JsWidget.Image(
            source = JsImageSource.Archive("wide.png"),
            fit = JsImageFit.COVER,
            modifiers = listOf(JsModifier.Size(width = 40)),
        )

        val drawn = drawPicture(80, 40, image)

        assertEquals(listOf(40, 20), listOf(drawn.width().inDp(), drawn.height().inDp()))
    }

    /** Draws `image` showing a red picture of this many pixels, and returns where it landed. */
    private fun drawPicture(width: Int, height: Int, image: JsWidget.Image): android.graphics.Rect {
        val picture = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        picture.eraseColor(android.graphics.Color.RED)
        draw(image, images = JsImageResolver { picture })
        compose.waitUntil(timeoutMillis = 5_000) { boundsOf(pureRed) != null }

        return checkNotNull(boundsOf(pureRed))
    }

    private fun draw(widget: JsWidget, images: JsImageResolver = JsImageResolver { null }) {
        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                CompositionLocalProvider(LocalJsImageResolver provides images) {
                    Box(Modifier.size(100.dp).background(Color.Black)) {
                        JsWidgetRenderer(widget = widget, jsEventHandler = { _, _ -> })
                    }
                }
            }
        }
    }

    /** Where the pixels of `colour` lie, in pixels, or null if none were drawn. */
    private fun boundsOf(colour: Color): android.graphics.Rect? {
        val pixels = compose.onRoot().captureToImage().toPixelMap()
        var found: android.graphics.Rect? = null
        for (y in 0 until pixels.height) {
            for (x in 0 until pixels.width) {
                if (pixels[x, y].isCloseTo(colour)) {
                    found = (found ?: android.graphics.Rect(x, y, x, y)).apply { union(x, y) }
                }
            }
        }
        return found?.apply { right += 1; bottom += 1 }
    }

    private fun Int.inDp(): Int = Math.round(this / compose.density.density)

    private fun Color.isCloseTo(other: Color) =
        abs(red - other.red) < 0.05f && abs(green - other.green) < 0.05f && abs(blue - other.blue) < 0.05f

    private fun block(color: JsColor) = JsWidget.Box(
        modifiers = listOf(JsModifier.Size(width = 20, height = 20), JsModifier.Background(color)),
    )

    private val success = Color(red = 34, green = 197, blue = 94)
    private val pureRed = Color(red = 255, green = 0, blue = 0)
}
