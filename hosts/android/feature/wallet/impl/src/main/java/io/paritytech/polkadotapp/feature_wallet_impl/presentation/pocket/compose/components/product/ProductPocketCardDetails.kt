package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product

import android.webkit.WebView
import androidx.activity.compose.BackHandler
import androidx.compose.animation.EnterExitState
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.layout.layout
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.ProductWebViewHost
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.LocalNavAnimatedVisibilityScope
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketCardSharedElement
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlin.math.roundToInt
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The expanded card: the same card element the list drew, moved to the top, with the product filling
 * the screen under it. The face keeps streaming up here, so the card stays live while it is open.
 */
@Composable
fun ProductPocketCardDetails(
    modifier: Modifier = Modifier,
    card: PocketCardUiModel.ProductCard,
    bindings: ProductFaceBindings,
    session: SpaHostSession?,
    openingFaceShown: Boolean?,
    cardIndex: Int,
    onSettled: () -> Unit,
    onBack: () -> Unit,
) {
    BackHandler { onBack() }

    val fold = rememberExpandedCardFoldState()

    // The product is asked for only once the card has arrived, so its WebView is not built while the
    // card is still travelling. The face folds away after the arrival, so the shared-element
    // transition lands on the card's real bounds, and before the page loads, so building the WebView
    // does not starve that animation. Applied once: a recreated screen must not fold away a face
    // the user has pulled back.
    val arrival = LocalNavAnimatedVisibilityScope.current?.transition
    val arrived = arrival == null || arrival.currentState == EnterExitState.Visible
    var openingFaceApplied by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(arrived, openingFaceShown) {
        if (!arrived || openingFaceShown == null) return@LaunchedEffect

        if (!openingFaceApplied) {
            if (!openingFaceShown) fold.showFace(false)
            openingFaceApplied = true
        }
        onSettled()
    }

    LaunchedEffect(session, fold) {
        session?.faceShownRequests?.collect { request ->
            launch { request.reply.complete(fold.showFace(request.shown)) }
        }
    }

    Column(modifier = modifier.fillMaxSize()) {
        PolkadotTopBar(
            title = card.title,
            navigationAction = rememberTopBarAction(onBack),
            titleAlignment = TopBarTitleAlignment.Center
        )

        FoldableCard(fold = fold) {
            ProductPocketCard(
                modifier = Modifier
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                    .pocketCardSharedElement(cardIndex),
                card = card,
                bindings = bindings,
                onOpen = null,
                onRemoveRequested = null
            )

            VerticalSpacer { mediumIncreased }
        }

        ExpandedProductContent(
            modifier = Modifier.fillMaxSize(),
            session = session
        )
    }
}

/**
 * The card above the product, draggable out of the way.
 *
 * Reporting a shorter height as it folds is what grows the product: the column hands on whatever the
 * card stops asking for. The drag lives here rather than on the product, because the product is a
 * WebView and forwards no scrolling to Compose.
 */
@Composable
private fun FoldableCard(
    fold: ExpandedCardFoldState,
    content: @Composable () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clipToBounds()
            .layout { measurable, constraints ->
                val placeable = measurable.measure(constraints)
                fold.maxFoldPx = placeable.height.toFloat()
                val shown = (placeable.height - fold.foldedPx).roundToInt().coerceAtLeast(0)

                layout(placeable.width, shown) {
                    placeable.place(0, -fold.foldedPx.roundToInt())
                }
            }
            .draggable(
                orientation = Orientation.Vertical,
                state = rememberDraggableState { delta -> fold.drag(delta) },
                onDragStarted = { fold.beginDrag() },
                onDragStopped = { fold.endDrag() },
            ),
        content = { content() },
    )
}

@Composable
private fun ExpandedProductContent(
    modifier: Modifier = Modifier,
    session: SpaHostSession?,
) {
    // Stand-ins for a product that has not been asked for yet, so the card still has ground under it
    // on its way up. Remembered unconditionally: a composable cannot remember only sometimes.
    val noProgressYet = remember { MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle) }
    val noWebViewYet = remember { MutableStateFlow<WebView?>(null) }

    val loadProgress by (session?.loadProgress ?: noProgressYet).collectAsStateWithLifecycle()
    val webView by (session?.webView ?: noWebViewYet).collectAsStateWithLifecycle()

    // Once the product has painted once it owns the space; its own navigations must not blank it.
    var productShowing by remember(session) { mutableStateOf(false) }
    LaunchedEffect(loadProgress) {
        if (loadProgress == DotNsLoadProgress.Completed) productShowing = true
    }

    // The app's own ground under the card until the product has something to show, so a page that
    // paints white does not flash through the arrival.
    Box(
        modifier = modifier.background(PolkadotTheme.colors.bg.surface.main),
        contentAlignment = Alignment.Center,
    ) {
        when {
            // The product lays itself out into the viewport it is given. Left running under the app's
            // navigation bar, a page built to fit `100vh` hides its last rows behind it with nothing
            // to scroll, since the page is the size of the viewport by construction.
            productShowing -> ProductWebViewHost(
                modifier = Modifier
                    .fillMaxSize()
                    .windowInsetsPadding(LocalAppNavigationBarInsets.current),
                webView = webView,
            )

            loadProgress is DotNsLoadProgress.Failed -> NovaText(
                text = stringResource(RCommon.string.product_resolution_error_unknown),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary
            )

            else -> ProductLoadProgress(progress = loadProgress)
        }
    }
}

/**
 * A product can take seconds to fetch on its first open, and the space below the card is empty for
 * all of it. Idle counts as loading here: the card is open, so the product is coming, and dotNS
 * reports nothing until its own lookup returns.
 */
@Composable
private fun ProductLoadProgress(progress: DotNsLoadProgress) {
    val downloaded = (progress as? DotNsLoadProgress.Downloading)?.fraction
    if (downloaded == null) {
        NovaCircularProgressIndicator(modifier = Modifier.size(PRODUCT_PROGRESS_SIZE))
    } else {
        NovaCircularProgressIndicator(modifier = Modifier.size(PRODUCT_PROGRESS_SIZE), progress = { downloaded })
    }
}

private val PRODUCT_PROGRESS_SIZE = 48.dp
