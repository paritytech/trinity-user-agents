@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.updateTransition
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.IntOffset
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.compose.TickerWordmark
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.statusBar.reportsScrollToStatusBar
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDownwards
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpwards
import io.paritytech.polkadotapp.design.components.icon.vectors.Clock
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.blurEffect
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.DigitalDollarCardDetailsViewModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageScene
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.debug.CoinageTestDataMode
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.LocalPocketScreenTransition
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.rememberCardCloseDragState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketCardSharedElement
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketContentSlide
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.BalanceRestoreUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageBalanceBreakdownUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.DigitalDollarCardDetailsUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketScreenState
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf
import kotlin.math.roundToInt
import io.paritytech.polkadotapp.common.R as RCommon

private val BalancedLineBreak = LineBreak(
    strategy = LineBreak.Strategy.Balanced,
    strictness = LineBreak.Strictness.Normal,
    wordBreak = LineBreak.WordBreak.Default
)

@Composable
fun DigitalDollarCardDetails(
    card: PocketCardUiModel.DigitalDollar,
    onBack: () -> Unit,
    cardIndex: Int
) {
    val viewModel = hiltViewModel<DigitalDollarCardDetailsViewModel>()
    val loadingState by viewModel.coinageState.collectAsStateWithLifecycle()
    val state by viewModel.state.collectAsStateWithLifecycle()
    val fundingActivity by viewModel.fundingActivity.collectAsStateWithLifecycle()
    val fundingSheet by viewModel.fundingSheet.collectAsStateWithLifecycle()

    BackHandler { onBack() }

    DigitalDollarCardDetailsContent(
        card = card,
        onBack = onBack,
        cardIndex = cardIndex,
        coinageLoadingState = loadingState,
        state = state,
        showsFundingHistory = viewModel.showsFundingHistory,
        onSendClick = viewModel::onSendClick,
        onHistoryClick = viewModel::onHistoryClick,
        onGetCashClick = viewModel::onGetCashClick,
        onAutoFundClick = viewModel::onAutoFundClick,
        onCoinTapped = viewModel::onCoinTapped,
        onCoinsFlickedDown = viewModel::onCoinsFlickedDown,
        onHeadingClicked = viewModel::onHeadingClicked,
        onTestDataSelected = viewModel::onTestDataSelected,
        onCoinSetSelected = viewModel::onCoinSetSelected,
        onShareLogsClick = viewModel::onShareLogsClick,
        onBackupUpdateClick = viewModel::onBackupUpdateClick,
        onBackupCloseClick = viewModel::onBackupCloseClick
    )

    FundingActivitySheet(
        sheet = fundingSheet,
        state = fundingActivity,
        onHistory = viewModel::onHistoryClick,
        onBack = viewModel::onSheetBack,
        onStartNew = viewModel::onNewTopUpClick,
        onRowClick = viewModel::onFundingRowClick,
        onDismiss = viewModel::onSheetDismiss,
    )
}

@Composable
private fun DigitalDollarCardDetailsContent(
    card: PocketCardUiModel.DigitalDollar,
    onBack: () -> Unit,
    cardIndex: Int,
    coinageLoadingState: LoadingState<CoinageUiState>,
    state: DigitalDollarCardDetailsUiState,
    showsFundingHistory: Boolean,
    onSendClick: () -> Unit,
    onHistoryClick: () -> Unit,
    onGetCashClick: () -> Unit,
    onAutoFundClick: () -> Unit,
    onCoinTapped: (String?) -> Unit,
    onCoinsFlickedDown: () -> Unit,
    onHeadingClicked: () -> Unit,
    onTestDataSelected: (CoinageTestDataMode) -> Unit,
    onCoinSetSelected: (CoinageScene.CoinSet) -> Unit,
    onShareLogsClick: () -> Unit,
    onBackupUpdateClick: () -> Unit,
    onBackupCloseClick: () -> Unit
) {
    val closeDrag = rememberCardCloseDragState()

    val scrollState = rememberScrollState()
    val pageAtTop by remember { derivedStateOf { scrollState.value == 0 } }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .reportsScrollToStatusBar(scrollState)
            .verticalScroll(scrollState)
            .windowInsetsPadding(WindowInsets.statusBars.only(WindowInsetsSides.Top))
            .padding(PolkadotTheme.spacings.mediumIncreased)
            .windowInsetsPadding(LocalAppNavigationBarInsets.current)
    ) {
        // Where iOS has its navigation bar's history item: the top end, above the card.
        if (showsFundingHistory) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                PolkadotIconButton(
                    icon = NovaIcons.Clock,
                    onClick = onHistoryClick,
                    style = PolkadotButtonStyle.ghost(),
                    size = PolkadotIconButtonSize.medium(),
                    shape = PolkadotButtonShape.pill,
                )
            }

            VerticalSpacer { small }
        }

        DigitalDollarCard(
            modifier = Modifier
                .offset { IntOffset(0, closeDrag.offsetPx.roundToInt()) }
                .pocketCardSharedElement(cardIndex)
                .draggable(
                    orientation = Orientation.Vertical,
                    enabled = pageAtTop,
                    state = rememberDraggableState(closeDrag::drag),
                    onDragStarted = { closeDrag.beginDrag() },
                    onDragStopped = { closeDrag.release(onBack) }
                ),
            card = card,
            isExpanded = true
        )

        VerticalSpacer { mediumIncreased }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .pocketContentSlide()
                .graphicsLayer {
                    translationY = closeDrag.offsetPx
                    alpha = 1f - closeDrag.progress
                    renderEffect = blurEffect(closeDrag.blurSigma, density)
                }
        ) {
            AnimatedContent(
                targetState = state.balanceRestore,
                contentKey = { it::class.simpleName }
            ) { balanceRestoreState ->
                when (balanceRestoreState) {
                    BalanceRestoreUiState.NotDetermined -> Unit

                    is BalanceRestoreUiState.SendCash -> SendCashActions(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(IntrinsicSize.Min),
                        canGetCash = balanceRestoreState.canGetCash,
                        onGetCashClick = onGetCashClick,
                        onSendClick = onSendClick
                    )

                    is BalanceRestoreUiState.Restore -> {
                        var isDoneUpdatingVisible by remember { mutableStateOf(false) }
                        var isWhyVisible by remember { mutableStateOf(false) }

                        BalanceRestoredWidget(
                            modifier = Modifier.fillMaxWidth(),
                            isInProgress = balanceRestoreState.inProgress,
                            onWhyClick = { isWhyVisible = true },
                            onCloseClick = { isDoneUpdatingVisible = true },
                            onUpdateClick = onBackupUpdateClick
                        )

                        DoneUpdatingBottomSheet(
                            isVisible = isDoneUpdatingVisible,
                            onDismissRequest = { isDoneUpdatingVisible = false },
                            onConfirm = onBackupCloseClick
                        )

                        WhyUpdateBottomSheet(
                            isVisible = isWhyVisible,
                            onDismissRequest = { isWhyVisible = false }
                        )
                    }
                }
            }

            coinageLoadingState.onLoaded { coinageState ->
                VerticalSpacer { mediumIncreased }

                CoinageCardContent(
                    state = coinageState,
                    onAutoFundClick = onAutoFundClick,
                    onCoinTapped = onCoinTapped,
                    onCoinsFlickedDown = onCoinsFlickedDown,
                    onHeadingClicked = onHeadingClicked,
                    onShareLogsClick = onShareLogsClick,
                    onTestDataSelected = onTestDataSelected,
                    onCoinSetSelected = onCoinSetSelected
                )
            }
        }
    }
}

@Composable
private fun SendCashActions(
    modifier: Modifier = Modifier,
    canGetCash: Boolean,
    onGetCashClick: () -> Unit,
    onSendClick: () -> Unit
) {
    val assetSymbol = LocalPaymentAssetBrand.current.symbol

    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
    ) {
        if (canGetCash) CashActionButton(
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight(),
            icon = NovaIcons.ArrowUpwards,
            title = stringResource(RCommon.string.pocket_action_get),
            description = stringResource(RCommon.string.pocket_action_get_description, assetSymbol)
                .withCurrencyTickerStyle(PolkadotTheme.typography.body.small, TickerWordmark.Capital),
            onClick = onGetCashClick
        )

        CashActionButton(
            modifier = Modifier
                .weight(1f)
                .fillMaxHeight(),
            icon = NovaIcons.ArrowDownwards,
            title = stringResource(RCommon.string.common_send),
            description = stringResource(RCommon.string.pocket_action_send_description, assetSymbol)
                .withCurrencyTickerStyle(PolkadotTheme.typography.body.small, TickerWordmark.Capital),
            onClick = onSendClick
        )
    }
}

@Composable
private fun CashActionButton(
    modifier: Modifier = Modifier,
    icon: ImageVector,
    title: String,
    description: AnnotatedString,
    onClick: () -> Unit
) {
    PolkadotButton(
        onClick = onClick,
        modifier = modifier,
        shape = PolkadotTheme.shapes.mediumIncreased
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
        ) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall),
                verticalAlignment = Alignment.CenterVertically
            ) {
                NovaIcon(imageVector = icon)

                NovaText(text = title)
            }

            NovaText(
                text = description,
                style = PolkadotTheme.typography.body.small.copy(lineBreak = BalancedLineBreak),
                color = PolkadotTheme.colors.fg.secondaryInverted,
                textAlign = TextAlign.Center
            )
        }
    }
}

@Preview
@Composable
private fun DigitalDollarCardDetailsPreview() {
    val card = PocketCardUiModel.DigitalDollar(
        amounts = LoadingState.Loaded(
            PocketCardUiModel.DigitalDollar.Amounts(TokenAmountModel.mock, TokenAmountModel.mock)
        ),
        syncInProgress = false,
    )

    PolkadotTheme {
        CompositionLocalProvider(
            LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked,
            LocalPocketScreenTransition provides updateTransition(PocketScreenState.CardDetails(card), label = "preview")
        ) {
            DigitalDollarCardDetailsContent(
                card = card,
                onBack = {},
                cardIndex = 0,
                coinageLoadingState = LoadingState.Loaded(
                    CoinageUiState(
                        tokensState = CoinageUiState.TokensState(
                            totalBalance = TokenAmountModel.mock,
                            readyBalance = TokenAmountModel.mock,
                            clearingBalance = TokenAmountModel.mock,
                            coins = persistentListOf(),
                            traces = persistentMapOf(),
                            breakdown = CoinageBalanceBreakdownUiModel(
                                availablePrivate = TokenAmountModel.mock,
                                gainingPrivacy = TokenAmountModel.mock,
                                pending = TokenAmountModel.mock,
                                canSpendGainingPrivacy = true
                            )
                        ),
                        autoFundAvailable = true,
                        fundInProgress = false,
                        actionsEnabled = true,
                        shareLogsEnabled = true,
                        detailsVisible = false,
                        focusedCoin = null,
                        coinSet = CoinageScene.CoinSet.SHIPPED,
                        testData = CoinageTestDataMode.LIVE
                    )
                ),
                state = DigitalDollarCardDetailsUiState(
                    balanceRestore = BalanceRestoreUiState.SendCash(canGetCash = true)
                ),
                showsFundingHistory = true,
                onSendClick = {},
                onHistoryClick = {},
                onGetCashClick = {},
                onAutoFundClick = {},
                onCoinTapped = {},
                onCoinsFlickedDown = {},
                onHeadingClicked = {},
                onTestDataSelected = {},
                onCoinSetSelected = {},
                onShareLogsClick = {},
                onBackupUpdateClick = {},
                onBackupCloseClick = {}
            )
        }
    }
}
