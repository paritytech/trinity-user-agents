package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.SharedTransitionLayout
import androidx.compose.animation.core.Transition
import androidx.compose.animation.core.updateTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.layout
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.notification.rememberAppNotifier
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.screens.ObserveViewModelEvents
import io.paritytech.polkadotapp.design.components.dialog.NovaAlertDialog
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleSize
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_products_api.presentation.widget.JsImageResolver
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.PocketRank
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketTestTags
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.PocketViewModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.ProductFaceBindings
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.LocalCardTilt
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.rememberCardTilt
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.*
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.DigitalDollarCard
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.DigitalDollarCardDetails
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.id.IdCard
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.id.IdCardDetails
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.ProductPocketCard
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.ProductPocketCardDetails
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketScreenState
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import kotlinx.coroutines.flow.MutableStateFlow
import io.paritytech.polkadotapp.common.R as RCommon

private val CollectiblesSketchbookPeek = 80.dp

@Composable
fun PocketScreen() {
    val viewModel = hiltViewModel<PocketViewModel>()

    val screenState by viewModel.state.collectAsStateWithLifecycle()
    val cards by viewModel.cards.collectAsStateWithLifecycle()
    val expandedProductSession by viewModel.expandedProductSession.collectAsStateWithLifecycle()

    ObserveViewModelEvents(viewModel, rememberAppNotifier())

    PocketScreenInternal(
        screenState = screenState,
        cards = cards,
        expandedProductSession = expandedProductSession,
        bindingsOf = viewModel::bindingsOf,
        onCardSelected = viewModel::selectCard,
        onCardDismissed = viewModel::dismissCard,
        onShareId = viewModel::onShareId,
        onSketchbookSelected = viewModel::showCollectiblesSketchbook,
        onSketchbookDismissed = viewModel::hideCollectiblesSketchbook,
        onOpenCollectibles = viewModel::openCollectibles,
        onExpandedCardSettled = viewModel::hostExpandedProduct,
        onProductCardRemovalRequested = viewModel::requestRemoval,
        onRemovalConfirmed = viewModel::confirmRemoval,
        onRemovalDismissed = viewModel::dismissRemoval
    )
}

@Composable
private fun PocketScreenInternal(
    screenState: PocketScreenState,
    cards: ImmutableList<PocketCardUiModel>,
    expandedProductSession: SpaHostSession?,
    bindingsOf: (PocketCardUiModel.ProductCard) -> ProductFaceBindings,
    onCardSelected: (PocketCardUiModel) -> Unit,
    onCardDismissed: () -> Unit,
    onShareId: () -> Unit,
    onSketchbookSelected: () -> Unit,
    onSketchbookDismissed: () -> Unit,
    onOpenCollectibles: () -> Unit,
    onExpandedCardSettled: (PocketCardUiModel.ProductCard) -> Unit,
    onProductCardRemovalRequested: (PocketCardUiModel.ProductCard) -> Unit,
    onRemovalConfirmed: () -> Unit,
    onRemovalDismissed: () -> Unit
) {
    val listState = rememberLazyListState()
    val transition = updateTransition(screenState, label = "pocket_card_selection")
    val cardTilt = rememberCardTilt()

    PolkadotSurface {
        SharedTransitionLayout {
            CompositionLocalProvider(
                LocalSharedTransitionScope provides this,
                LocalCardTilt provides cardTilt
            ) {
                transition.AnimatedContent(
                    transitionSpec = { pocketFadeIn() togetherWith pocketFadeOut() },
                    contentKey = { it.contentKey }
                ) { current ->
                    CompositionLocalProvider(LocalNavAnimatedVisibilityScope provides this) {
                        when (current) {
                            is PocketScreenState.List -> {
                                PocketList(
                                    cards = cards,
                                    anchorCard = transition.extractAnchorCard(),
                                    listState = listState,
                                    collectiblesAvailable = current.collectiblesAvailable,
                                    bindingsOf = bindingsOf,
                                    onCardSelected = onCardSelected,
                                    onCollectiblesSelected = onSketchbookSelected,
                                    onProductCardRemovalRequested = onProductCardRemovalRequested
                                )

                                current.removalCandidate?.let { candidate ->
                                    RemoveCardDialog(
                                        card = candidate,
                                        onConfirm = onRemovalConfirmed,
                                        onDismiss = onRemovalDismissed
                                    )
                                }
                            }

                            is PocketScreenState.CardDetails -> {
                                SelectedCardDetails(
                                    selectedCard = current.selectedCard,
                                    allCards = cards,
                                    expandedProductSession = expandedProductSession,
                                    bindingsOf = bindingsOf,
                                    onSettled = onExpandedCardSettled,
                                    onBack = onCardDismissed,
                                    onShareId = onShareId,
                                )
                            }

                            is PocketScreenState.Collectibles -> {
                                PocketCollectibles(
                                    onBack = onSketchbookDismissed,
                                    onViewButtonClick = onOpenCollectibles
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SelectedCardDetails(
    selectedCard: PocketCardUiModel,
    allCards: ImmutableList<PocketCardUiModel>,
    expandedProductSession: SpaHostSession?,
    bindingsOf: (PocketCardUiModel.ProductCard) -> ProductFaceBindings,
    onSettled: (PocketCardUiModel.ProductCard) -> Unit,
    onBack: () -> Unit,
    onShareId: () -> Unit,
) {
    val cardIndex = allCards.indexOfFirst { it.id == selectedCard.id }
    when (selectedCard) {
        is PocketCardUiModel.DigitalDollar -> DigitalDollarCardDetails(
            card = selectedCard,
            onBack = onBack,
            cardIndex = cardIndex
        )

        is PocketCardUiModel.IdCard -> IdCardDetails(
            card = selectedCard,
            onBack = onBack,
            onShareClick = onShareId,
            cardIndex = cardIndex,
        )

        is PocketCardUiModel.ProductCard -> ProductPocketCardDetails(
            card = selectedCard,
            bindings = bindingsOf(selectedCard),
            session = expandedProductSession,
            cardIndex = cardIndex,
            onSettled = { onSettled(selectedCard) },
            onBack = onBack,
        )
    }
}

@Composable
private fun RemoveCardDialog(
    card: PocketCardUiModel.ProductCard,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit
) {
    NovaAlertDialog(
        modifier = Modifier.testTag(PocketTestTags.REMOVE_CARD_DIALOG),
        title = stringResource(RCommon.string.pocket_remove_card_title, card.title),
        text = stringResource(RCommon.string.pocket_remove_card_message),
        positiveButtonTitle = stringResource(RCommon.string.pocket_remove_card_action),
        onPositiveButtonClick = onConfirm,
        negativeButtonTitle = stringResource(RCommon.string.pocket_remove_card_cancel),
        onNegativeButtonClick = onDismiss,
        onDismissRequest = onDismiss
    )
}

@Composable
private fun PocketList(
    cards: ImmutableList<PocketCardUiModel>,
    anchorCard: PocketCardUiModel?,
    listState: LazyListState,
    collectiblesAvailable: Boolean,
    bindingsOf: (PocketCardUiModel.ProductCard) -> ProductFaceBindings,
    onCardSelected: (PocketCardUiModel) -> Unit,
    onCollectiblesSelected: () -> Unit,
    onProductCardRemovalRequested: (PocketCardUiModel.ProductCard) -> Unit
) {
    val anchorIndex = cards.indexOfFirst { it.id == anchorCard?.id }

    val navigationBarInsets = LocalAppNavigationBarInsets.current

    Box {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.pocket_toolbar_title),
                titleSize = TopBarTitleSize.Large
            )

            LazyColumn(
                state = listState,
                verticalArrangement = Arrangement.spacedBy(-CardSizes.OVERLAP),
                contentPadding = WindowInsets(
                    top = PolkadotTheme.spacings.mediumIncreased,
                    bottom = PolkadotTheme.spacings.mediumIncreased,
                    left = PolkadotTheme.spacings.mediumIncreased,
                    right = PolkadotTheme.spacings.mediumIncreased
                ).add(navigationBarInsets).asPaddingValues()
            ) {
                // Keyed: a product card holds a face subscription and its resolved images, so
                // positional identity would make every card below an insertion drop and re-subscribe.
                itemsIndexed(cards, key = { _, card -> card.id }) { index, card ->
                    val cardModifier = Modifier.pocketListCardSharedElement(
                        card = card,
                        index = index,
                        anchorCard = anchorCard,
                        anchorIndex = anchorIndex
                    )

                    when (card) {
                        is PocketCardUiModel.DigitalDollar -> {
                            DigitalDollarCard(
                                modifier = cardModifier,
                                card = card,
                                onSelected = onCardSelected,
                                isExpanded = false
                            )
                        }

                        is PocketCardUiModel.IdCard -> {
                            IdCard(
                                modifier = cardModifier,
                                card = card,
                                onSelected = onCardSelected,
                            )
                        }

                        is PocketCardUiModel.ProductCard -> {
                            ProductPocketCard(
                                modifier = cardModifier,
                                card = card,
                                bindings = bindingsOf(card),
                                onOpen = onCardSelected,
                                onRemoveRequested = onProductCardRemovalRequested
                            )
                        }
                    }
                }
            }
        }

        AnimatedVisibility(
            modifier = Modifier
                .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                .align(Alignment.BottomCenter)
                .layout { measurable, constraints ->
                    val placeable = measurable.measure(constraints)
                    val peek = navigationBarInsets.getBottom(this) + CollectiblesSketchbookPeek.roundToPx()
                    layout(placeable.width, peek) {
                        placeable.place(0, 0)
                    }
                }
                .pocketCollectiblesImageSharedElement(),
            visible = collectiblesAvailable,
            enter = fadeIn(),
            exit = fadeOut()
        ) {
            CollectiblesSketchbook(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable(onClick = onCollectiblesSelected),
                blackAndWhite = true,
                onViewButtonClick = {}
            )
        }
    }
}

private fun Transition<PocketScreenState>.extractAnchorCard(): PocketCardUiModel? =
    (targetState as? PocketScreenState.CardDetails)?.selectedCard
        ?: (currentState as? PocketScreenState.CardDetails)?.selectedCard

@Preview
@Composable
private fun PocketScreenPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
        ) {
            PocketScreenInternal(
                screenState = PocketScreenState.List(collectiblesAvailable = true, removalCandidate = null),
                cards = persistentListOf(
                    PocketCardUiModel.DigitalDollar(
                        amounts = LoadingState.Loaded(
                            PocketCardUiModel.DigitalDollar.Amounts(TokenAmountModel.mock, TokenAmountModel.mock)
                        ),
                        syncInProgress = false,
                        accountBackupPending = false,
                    ),
                    PocketCardUiModel.IdCard("username.99", "15oF4u...zaC1Ap", PocketRank.Basic),
                    PocketCardUiModel.ProductCard(
                        key = PocketCardKey(ProductId.fromStoredValue("peopl.dot"), PocketCardId("humanity")),
                        title = "Humanity",
                        pinned = true
                    )
                ),
                expandedProductSession = null,
                bindingsOf = { card ->
                    ProductFaceBindings(
                        face = MutableStateFlow(JsWidget.Text(text = card.title)),
                        onFaceAction = { _, _ -> },
                        imageResolver = JsImageResolver { null },
                    )
                },
                onExpandedCardSettled = {},
                onCardSelected = {},
                onCardDismissed = {},
                onShareId = {},
                onSketchbookSelected = {},
                onSketchbookDismissed = {},
                onOpenCollectibles = {},
                onProductCardRemovalRequested = {},
                onRemovalConfirmed = {},
                onRemovalDismissed = {}
            )
        }
    }
}
