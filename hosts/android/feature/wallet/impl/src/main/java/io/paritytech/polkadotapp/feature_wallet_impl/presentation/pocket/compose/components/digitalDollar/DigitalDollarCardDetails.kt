@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.compose.TickerWordmark
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDownwards
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpwards
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleSize
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.R
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.DigitalDollarCardDetailsViewModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketCardSharedElement
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketContentSlide
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.BalanceRestoreUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageBalanceBreakdownUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.DigitalDollarCardDetailsUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityDayUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingDayTitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf
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

    BackHandler { onBack() }

    DigitalDollarCardDetailsContent(
        card = card,
        onBack = onBack,
        cardIndex = cardIndex,
        coinageLoadingState = loadingState,
        state = state,
        fundingActivity = fundingActivity,
        onSendClick = viewModel::onSendClick,
        onFundingRowClick = viewModel::onFundingRowClick,
        onGetCashClick = viewModel::onGetCashClick,
        onAutoFundClick = viewModel::onAutoFundClick,
        onCoinTapped = viewModel::onCoinTapped,
        onCoinsFlickedDown = viewModel::onCoinsFlickedDown,
        onHeadingClicked = viewModel::onHeadingClicked,
        onShareLogsClick = viewModel::onShareLogsClick,
        onBackupUpdateClick = viewModel::onBackupUpdateClick,
        onBackupCloseClick = viewModel::onBackupCloseClick
    )
}

@Composable
private fun DigitalDollarCardDetailsContent(
    card: PocketCardUiModel.DigitalDollar,
    onBack: () -> Unit,
    cardIndex: Int,
    coinageLoadingState: LoadingState<CoinageUiState>,
    state: DigitalDollarCardDetailsUiState,
    fundingActivity: FundingActivityUiState,
    onSendClick: () -> Unit,
    onFundingRowClick: (id: String) -> Unit,
    onGetCashClick: () -> Unit,
    onAutoFundClick: () -> Unit,
    onCoinTapped: (String?) -> Unit,
    onCoinsFlickedDown: () -> Unit,
    onHeadingClicked: () -> Unit,
    onShareLogsClick: () -> Unit,
    onBackupUpdateClick: () -> Unit,
    onBackupCloseClick: () -> Unit
) {
    var inProgressExpanded by remember { mutableStateOf(false) }
    val showInProgressPanel = inProgressExpanded && fundingActivity.inFlight.size > 1
    val listState = rememberLazyListState()
    // The card is the list's first item: once it has scrolled away, the header shows it small and the sticky bar
    // takes over the balance.
    val cardCollapsed by remember { derivedStateOf { listState.firstVisibleItemIndex > 0 } }
    val dayOfItem = remember(fundingActivity.days) { fundingActivity.days.dayOfItemKey() }
    val passedDay by remember(dayOfItem) {
        derivedStateOf { listState.layoutInfo.visibleItemsInfo.firstOrNull()?.key?.let(dayOfItem::get) }
    }
    val navigationInsets = LocalAppNavigationBarInsets.current.asPaddingValues()
    val dockSpace = if (fundingActivity.inFlight.isEmpty()) 0.dp else DOCK_CLEARANCE

    Box(modifier = Modifier.fillMaxSize()) {
        Column(modifier = Modifier.fillMaxSize()) {
            Box(contentAlignment = Alignment.Center) {
                PolkadotTopBar(
                    title = if (cardCollapsed) "" else stringResource(RCommon.string.pocket_purse_title),
                    navigationAction = rememberTopBarAction(onBack),
                    titleAlignment = TopBarTitleAlignment.Center,
                    titleSize = TopBarTitleSize.Medium
                )
                if (cardCollapsed) CardThumbnail()
            }

            if (cardCollapsed) {
                StickyBalanceBar(day = passedDay?.let(fundingActivity.days::getOrNull), currentBalance = fundingActivity.currentBalance)
            }

            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                state = listState,
                contentPadding = PaddingValues(
                    start = PolkadotTheme.spacings.mediumIncreased,
                    end = PolkadotTheme.spacings.mediumIncreased,
                    top = PolkadotTheme.spacings.mediumIncreased,
                    bottom = navigationInsets.calculateBottomPadding() + dockSpace + PolkadotTheme.spacings.mediumIncreased,
                ),
            ) {
                item(key = CARD_KEY) {
                    DigitalDollarCard(
                        modifier = Modifier.pocketCardSharedElement(cardIndex),
                        card = card,
                        isExpanded = true
                    )
                }

                item(key = ACTIONS_KEY) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .pocketContentSlide()
                            .padding(top = PolkadotTheme.spacings.small)
                    ) {
                        AnimatedContent(
                            modifier = Modifier.zIndex(1f),
                            targetState = state.balanceRestore,
                            contentKey = { it::class.simpleName }
                        ) { balanceRestoreState ->
                            when (balanceRestoreState) {
                                BalanceRestoreUiState.NotDetermined -> Unit

                                is BalanceRestoreUiState.SendCash -> SendCashActions(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .height(ACTION_TILE_HEIGHT),
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

                        // The Total panel tucks under the tiles, which are drawn over it.
                        coinageLoadingState.onLoaded { coinageState ->
                            CoinageCardContent(
                                modifier = Modifier
                                    .offset(y = -TOTAL_TUCK)
                                    .padding(horizontal = TOTAL_INSET),
                                state = coinageState,
                                onAutoFundClick = onAutoFundClick,
                                onCoinTapped = onCoinTapped,
                                onCoinsFlickedDown = onCoinsFlickedDown,
                                onHeadingClicked = onHeadingClicked,
                                onShareLogsClick = onShareLogsClick
                            )
                        }

                        VerticalSpacer { large }
                    }
                }

                fundingHistory(days = fundingActivity.days, onRowClick = onFundingRowClick)
            }
        }

        if (showInProgressPanel) FundingInProgressScrim(onCollapse = { inProgressExpanded = false })

        val dockModifier = Modifier
            .align(Alignment.BottomCenter)
            .windowInsetsPadding(LocalAppNavigationBarInsets.current)
            .padding(horizontal = DOCK_INSET, vertical = DOCK_INSET)

        if (showInProgressPanel) {
            FundingInProgressPanel(
                modifier = dockModifier,
                rows = fundingActivity.inFlight,
                onCollapse = { inProgressExpanded = false },
                onRowClick = onFundingRowClick,
            )
        } else if (fundingActivity.inFlight.isNotEmpty()) {
            FundingInProgressDock(
                modifier = dockModifier,
                rows = fundingActivity.inFlight,
                onExpand = { inProgressExpanded = true },
                onRowClick = onFundingRowClick,
            )
        }
    }
}

/** The card, small, in the header once the full card has scrolled away. */
@Composable
private fun CardThumbnail() {
    Image(
        modifier = Modifier
            .size(width = THUMBNAIL_WIDTH, height = THUMBNAIL_HEIGHT)
            .clip(PolkadotTheme.shapes.small),
        painter = painterResource(R.drawable.img_digital_dollar_card),
        contentDescription = null,
        contentScale = ContentScale.Crop,
    )
}

/**
 * Under the header once the card has scrolled away: "Current balance" until the list reaches a day before today,
 * then that day and the balance at its end.
 */
@Composable
private fun StickyBalanceBar(day: FundingActivityDayUi?, currentBalance: String?) {
    val past = day?.takeIf { it.title != FundingDayTitle.Today }
    val label = past?.let { dayTitle(it.title) } ?: stringResource(RCommon.string.pocket_current_balance)
    val value = if (past != null) past.endOfDayBalance else currentBalance

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(STICKY_BAR_HEIGHT)
            .padding(horizontal = PolkadotTheme.spacings.large),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = label,
            style = PolkadotTheme.typography.title.small,
            color = PolkadotTheme.colors.fg.primary,
            maxLines = 1,
        )
        if (value != null) {
            NovaText(
                text = value,
                style = PolkadotTheme.typography.title.small,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
            )
        }
    }
}

/** Each history item's key to the index of its day, so the sticky bar knows which day is at the top. */
private fun List<FundingActivityDayUi>.dayOfItemKey(): Map<Any, Int> = buildMap {
    this@dayOfItemKey.forEachIndexed { index, day ->
        put(fundingDayKey(index), index)
        day.rows.forEach { row -> put(fundingRowKey(row.id), index) }
    }
}

private const val CARD_KEY = "card"
private const val ACTIONS_KEY = "actions"
private val ACTION_TILE_HEIGHT = 96.dp
private val TOTAL_TUCK = 8.dp
private val TOTAL_INSET = 3.dp
private val DOCK_INSET = 16.dp
private val DOCK_CLEARANCE = 120.dp
private val STICKY_BAR_HEIGHT = 49.dp
private val THUMBNAIL_WIDTH = 60.dp
private val THUMBNAIL_HEIGHT = 39.dp

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
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
        ) {
            DigitalDollarCardDetailsContent(
                card = PocketCardUiModel.DigitalDollar(
                    amounts = LoadingState.Loaded(
                        PocketCardUiModel.DigitalDollar.Amounts(TokenAmountModel.mock, TokenAmountModel.mock)
                    ),
                    syncInProgress = false,
                    accountBackupPending = false,
                ),
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
                        focusedCoin = null
                    )
                ),
                state = DigitalDollarCardDetailsUiState(
                    balanceRestore = BalanceRestoreUiState.SendCash(canGetCash = true)
                ),
                fundingActivity = FundingActivityUiState(inFlight = persistentListOf(), days = persistentListOf(), currentBalance = null),
                onSendClick = {},
                onFundingRowClick = {},
                onGetCashClick = {},
                onAutoFundClick = {},
                onCoinTapped = {},
                onCoinsFlickedDown = {},
                onHeadingClicked = {},
                onShareLogsClick = {},
                onBackupUpdateClick = {},
                onBackupCloseClick = {}
            )
        }
    }
}
