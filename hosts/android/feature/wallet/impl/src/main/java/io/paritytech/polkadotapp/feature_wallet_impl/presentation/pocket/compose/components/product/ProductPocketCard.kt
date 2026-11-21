package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import android.graphics.Bitmap
import android.view.ViewTreeObserver
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.rememberGraphicsLayer
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.parity.truapi.MaterialsResolution
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketFace
import io.paritytech.polkadotapp.feature_products_api.model.JsTypographyStyle
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsWidgetRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsImageResolver
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.LocalJsMaterialsRenderer
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.PocketCardTheme
import io.paritytech.polkadotapp.feature_wallet_impl.R
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.CardSizes
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.PocketCardColors
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials.CardMaterialsSlot
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials.MaterialsGl
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials.PocketMaterials
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.withContext
import uniffi.truapi.ProductId
import kotlin.time.Duration.Companion.milliseconds

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
                        val resolution = face.materials
                        if (resolution != null && bindings.materials.drawsMaterialsOf(resolution)) {
                            MaterialsFace(face.widget, resolution, bindings)
                        } else {
                            JsWidgetRenderer(
                                widget = face.widget,
                                modifier = Modifier.matchParentSize(),
                                jsEventHandler = bindings.onFaceAction,
                            )
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

@Composable
private fun CardMaterialsSlot.drawsMaterialsOf(resolution: MaterialsResolution): Boolean {
    val resolved by resolution.resolved.collectAsState()
    return remember(resolved) { drawsMaterials(resolved) }
}

@Composable
private fun BoxScope.MaterialsFace(face: JsWidget, resolution: MaterialsResolution, bindings: ProductFaceBindings) {
    val context = LocalContext.current
    val view = LocalView.current
    val materials = remember(resolution) {
        MaterialsGl.start(context)
        bindings.materials.of(resolution, MaterialsGl.guard, MaterialsGl.allPlain)
    }
    val plainCard = rememberGraphicsLayer()
    val redrawn = remember { Channel<Unit>(Channel.CONFLATED) }
    val cardPicture = remember { mutableStateOf<ImageBitmap?>(null) }
    val cardCoordinates = remember { mutableStateOf<LayoutCoordinates?>(null) }

    // A child of the copy with a layer of its own redraws without running the copy's draw, so every window
    // draw asks for a capture; the rate bound and the pixel check keep a drawing shader from looping on it.
    DisposableEffect(view) {
        val listener = ViewTreeObserver.OnDrawListener { redrawn.trySend(Unit) }
        view.viewTreeObserver.addOnDrawListener(listener)
        onDispose { view.viewTreeObserver.removeOnDrawListener(listener) }
    }

    LaunchedEffect(plainCard) {
        redrawn.receiveAsFlow().collect {
            // The copy has a size only once it has drawn, and a prefetched card is composed before it draws.
            if (plainCard.size.width == 0 || plainCard.size.height == 0) return@collect
            val captured = runCancellableCatching { plainCard.toImageBitmap() }
                .logFailure("materials: the plain card was not captured")
                .getOrNull() ?: return@collect
            val shown = cardPicture.value?.asAndroidBitmap()
            val changed = withContext(Dispatchers.Default) {
                captured.asAndroidBitmap().copy(Bitmap.Config.ARGB_8888, false)?.takeUnless { shown?.sameAs(it) == true }
            }
            if (changed != null) cardPicture.value = changed.asImageBitmap()
            delay(BASE_CAPTURE_INTERVAL)
        }
    }

    Box(
        modifier = Modifier
            .matchParentSize()
            .clearAndSetSemantics {}
            .focusProperties { canFocus = false }
            .onGloballyPositioned { cardCoordinates.value = it }
            .drawWithContent { plainCard.record { this@drawWithContent.drawContent() } }
            .background(PocketCardColors.DigitalDollarCardBackground)
    ) {
        CardGrain()
        JsWidgetRenderer(widget = face, modifier = Modifier.matchParentSize(), jsEventHandler = { _, _ -> })
    }

    val renderer = remember(materials) { PocketMaterials(materials, cardPicture, cardCoordinates) }
    CompositionLocalProvider(LocalJsMaterialsRenderer provides renderer) {
        JsWidgetRenderer(widget = face, modifier = Modifier.matchParentSize(), jsEventHandler = bindings.onFaceAction)
    }
}

private val BASE_CAPTURE_INTERVAL = 250.milliseconds

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
                materials = CardMaterialsSlot(),
            ),
            onOpen = {},
            onRemoveRequested = {}
        )
    }
}
