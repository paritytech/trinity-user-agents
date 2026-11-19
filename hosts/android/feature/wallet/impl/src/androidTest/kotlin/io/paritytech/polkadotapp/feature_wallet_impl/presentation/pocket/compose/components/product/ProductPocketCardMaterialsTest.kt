package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFace
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials.CardMaterialsSlot
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials.layer
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.flow.MutableStateFlow
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.NodeMaterials
import uniffi.truapi.ProductId
import uniffi.truapi.ResolvedMaterials

@RunWith(AndroidJUnit4::class)
class ProductPocketCardMaterialsTest {
    @get:Rule
    val compose = createComposeRule()

    // A list composes the card it prefetches before it draws it, while the window keeps drawing.
    @Test
    fun aMaterialsCardThatHasNotDrawnYetDoesNotCrash() {
        val resolution = mockk<MaterialsResolution>(relaxed = true) {
            every { resolved } returns MutableStateFlow(
                ResolvedMaterials(listOf(NodeMaterials.Ready(listOf(layer(7, SOLID)))), noiseSeed = 1u),
            )
        }
        var windowFrame by mutableIntStateOf(0)

        compose.setContent {
            PolkadotTheme(theme = PolkadotAppTheme.BerlinNight) {
                Column {
                    Box(Modifier.size(10.dp).drawBehind { drawRect(if (windowFrame % 2 == 0) Color.Red else Color.Blue) })
                    Layout(content = { MaterialsCard(resolution) }) { measurables, constraints ->
                        measurables.forEach { it.measure(constraints) }
                        layout(constraints.maxWidth, 0) {}
                    }
                }
            }
        }
        repeat(WINDOW_FRAMES) {
            compose.runOnIdle { windowFrame++ }
            compose.waitForIdle()
        }

        compose.onNodeWithTag(PocketTestTags.PRODUCT_CARD).assertExists()
    }

    @Composable
    private fun MaterialsCard(resolution: MaterialsResolution) {
        ProductPocketCard(
            card = PocketCardUiModel.ProductCard(
                key = PocketCardKey(ProductId.parse("pocketdemo.dot").getOrThrow(), PocketCardId("loyalty")),
                title = "Loyalty",
                pinned = false,
            ),
            bindings = ProductFaceBindings(
                face = MutableStateFlow(PocketFace(JsWidget.Text(text = "Loyalty"), resolution)),
                onFaceAction = { _, _ -> },
                imageResolver = JsImageResolver { null },
                materials = CardMaterialsSlot(),
            ),
            onOpen = null,
            onRemoveRequested = null,
        )
    }

    private companion object {
        const val SOLID = "vec4 material(vec2 uv){return vec4(1.0, 0.0, 0.0, 1.0);}"
        const val WINDOW_FRAMES = 5
    }
}
