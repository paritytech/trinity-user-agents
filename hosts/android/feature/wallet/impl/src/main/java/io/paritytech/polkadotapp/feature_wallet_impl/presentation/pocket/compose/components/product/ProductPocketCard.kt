package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_wallet_impl.R
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.CardSizes
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.PocketCardColors
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.flow.MutableStateFlow

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
            Image(
                modifier = Modifier.matchParentSize(),
                painter = painterResource(R.drawable.img_texture_grain_dark),
                contentDescription = null,
                contentScale = ContentScale.Crop
            )

            currentFace?.let { widget ->
                CompositionLocalProvider(LocalJsImageResolver provides bindings.imageResolver) {
                    JsWidgetRenderer(
                        widget = widget,
                        modifier = Modifier.matchParentSize(),
                        jsEventHandler = bindings.onFaceAction,
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun ProductPocketCardPreview() {
    PolkadotTheme {
        ProductPocketCard(
            card = PocketCardUiModel.ProductCard(
                key = PocketCardKey(ProductId.fromStoredValue("game.dot"), PocketCardId("loyalty")),
                title = "Loyalty",
                pinned = false
            ),
            bindings = ProductFaceBindings(
                face = MutableStateFlow(JsWidget.Text(text = "Loyalty", style = JsTypographyStyle.HEADLINE_LARGE)),
                onFaceAction = { _, _ -> },
                imageResolver = JsImageResolver { null },
            ),
            onOpen = {},
            onRemoveRequested = {}
        )
    }
}
