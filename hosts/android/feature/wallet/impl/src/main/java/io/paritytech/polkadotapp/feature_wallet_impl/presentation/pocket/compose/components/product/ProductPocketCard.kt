package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFace
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.PocketCardTheme
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials.CardMaterialsSlot
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials.MaterialsFace
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials.MaterialsGl
import io.paritytech.polkadotapp.feature_wallet_impl.R
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.CardSizes
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.PocketCardColors
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.flow.MutableStateFlow
import uniffi.truapi.ProductId

/**
 * A product-backed card: the frame is the host's, the face inside it is the product's tree. The
 * face flow is collected only while this card is composed, which is what keeps its worker running.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun ProductPocketCard(
    modifier: Modifier = Modifier,
    card: PocketCardUiModel.ProductCard,
    bindings: ProductFaceBindings,
    onOpen: ((PocketCardUiModel.ProductCard) -> Unit)?,
    onRemoveRequested: ((PocketCardUiModel.ProductCard) -> Unit)?,
) {
    val currentFace by bindings.face.collectAsStateWithLifecycle()

    PocketCardTheme {
        PolkadotSurface(
            modifier = modifier.testTag(PocketTestTags.PRODUCT_CARD),
            shape = PolkadotTheme.shapes.large,
            color = PocketCardColors.DigitalDollarCardBackground,
            border = BorderStroke(Dp.Hairline, PocketCardColors.Secondary),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(CardSizes.HEIGHT)
                    // The expanded copy is the card the user is already looking at: it takes no presses
                    // of its own, so it does not ripple under the finger either.
                    .combinedClickable(
                        enabled = onOpen != null || onRemoveRequested != null,
                        onClick = { onOpen?.invoke(card) },
                        // A pinned card is never offered for removal, so long-press does nothing on it.
                        onLongClick = if (card.pinned) null else onRemoveRequested?.let { { it(card) } },
                    )
            ) {
                CardGrain()

                currentFace?.let { face ->
                    CompositionLocalProvider(LocalJsImageResolver provides bindings.imageResolver) {
                        MaterialsFace(
                            face = face.widget,
                            resolution = face.materials,
                            materials = bindings.materials,
                            jsEventHandler = bindings.onFaceAction,
                            modifier = Modifier.fillMaxSize(),
                        ) {
                            Box(Modifier.matchParentSize().background(PocketCardColors.DigitalDollarCardBackground))
                            CardGrain()
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun BoxScope.CardGrain() {
    Image(
        modifier = Modifier.matchParentSize(),
        painter = painterResource(R.drawable.img_texture_grain_dark),
        contentDescription = null,
        contentScale = ContentScale.Crop
    )
}

@Preview
@Composable
private fun ProductPocketCardPreview() {
    PolkadotTheme {
        ProductPocketCard(
            card = PocketCardUiModel.ProductCard(
                key = PocketCardKey(ProductId.parse("game.dot").getOrThrow(), PocketCardId("loyalty")),
                title = "Loyalty",
                pinned = false
            ),
            bindings = ProductFaceBindings(
                face = MutableStateFlow(
                    PocketFace(JsWidget.Text(text = "Loyalty", style = JsTypographyStyle.HEADLINE_LARGE), materials = null)
                ),
                onFaceAction = { _, _ -> },
                imageResolver = JsImageResolver { null },
                materials = CardMaterialsSlot(
                    gl = MaterialsGl(LocalContext.current),
                    loadImage = { Result.failure(UnsupportedOperationException()) },
                    scope = rememberCoroutineScope(),
                ),
            ),
            onOpen = {},
            onRemoveRequested = {}
        )
    }
}
