package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.toPixelMap
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsButtonVariant
import io.paritytech.polkadotapp.feature_products_api.model.JsColor
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ProductPocketCardThemeTest {
    @get:Rule
    val compose = createComposeRule()

    private var pickedTheme by mutableStateOf(PolkadotAppTheme.BerlinNight)

    private val faceUsingThemeDefaults = JsWidget.Column(
        modifiers = listOf(JsModifier.FillMaxWidth(), JsModifier.FillMaxHeight()),
        children = listOf(
            JsWidget.Text(text = "Loyalty"),
            JsWidget.Button(text = "Remove", variant = JsButtonVariant.TEXT),
            JsWidget.Button(text = "Open", variant = JsButtonVariant.SECONDARY),
            JsWidget.Box(
                modifiers = listOf(
                    JsModifier.FillMaxWidth(),
                    JsModifier.FillMaxHeight(),
                    JsModifier.Background(color = JsColor.FG_PRIMARY),
                ),
            ),
        ),
    )

    @Test
    fun aProductCardIsDrawnInBerlinNightWhateverThemeTheUserPicked() {
        compose.setContent {
            PolkadotTheme(theme = pickedTheme) {
                ProductPocketCard(
                    card = PocketCardUiModel.ProductCard(
                        key = PocketCardKey(ProductId.fromStoredValue("pocketdemo.dot"), PocketCardId("loyalty")),
                        title = "Loyalty",
                        pinned = false,
                    ),
                    bindings = ProductFaceBindings(
                        face = MutableStateFlow(faceUsingThemeDefaults),
                        onFaceAction = { _, _ -> },
                        imageResolver = JsImageResolver { null },
                    ),
                    onOpen = null,
                    onRemoveRequested = null,
                )
            }
        }

        val inBerlinNight = captureCardUnder(PolkadotAppTheme.BerlinNight)
        val ground = inBerlinNight.toPixelMap().let { it[it.width / 2, it.height * 9 / 10] }
        assertEquals("the FgPrimary the Pocket RFC promises", Color(0xFFF4F4F5), ground)

        PolkadotAppTheme.entries.forEach { theme ->
            assertArrayEquals(theme.name, inBerlinNight.pixels(), captureCardUnder(theme).pixels())
        }
    }

    private fun captureCardUnder(theme: PolkadotAppTheme): ImageBitmap {
        pickedTheme = theme
        return compose.onNodeWithTag(PocketTestTags.PRODUCT_CARD).captureToImage()
    }

    private fun ImageBitmap.pixels(): IntArray = IntArray(width * height).also { readPixels(it) }
}
