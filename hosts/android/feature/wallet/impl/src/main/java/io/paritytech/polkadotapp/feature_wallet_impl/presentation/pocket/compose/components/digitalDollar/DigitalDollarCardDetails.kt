@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDownward
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowUpward
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.DigitalDollarCardDetailsViewModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketCardSharedElement
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.pocketContentSlide
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.BalanceRestoreUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageBalanceBreakdownUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.DigitalDollarCardDetailsUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

private val TILE_ICON_SIZE = 20.dp

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
        onDetailsToggled = viewModel::onDetailsToggled,
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
    onDetailsToggled: () -> Unit,
    onShareLogsClick: () -> Unit,
    onBackupUpdateClick: () -> Unit,
    onBackupCloseClick: () -> Unit
) {
    var inProgressExpanded by remember { mutableStateOf(false) }
    val showInProgressPanel = inProgressExpanded && fundingActivity.inFlight.size > 1

    Box(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier.fillMaxSize()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.pocket_digital_dollar_title, LocalPaymentAssetBrand.current.symbol),
                navigationAction = rememberTopBarAction(onBack),
                titleAlignment = TopBarTitleAlignment.Center
            )

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
                    .padding(PolkadotTheme.spacings.mediumIncreased)
                    .windowInsetsPadding(LocalAppNavigationBarInsets.current)
            ) {
                DigitalDollarCard(
                    modifier = Modifier.pocketCardSharedElement(cardIndex),
                    card = card,
                    isExpanded = true
                )

                VerticalSpacer { mediumIncreased }

                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .pocketContentSlide()
                ) {
                    AnimatedContent(
                        targetState = state.balanceRestore,
                        contentKey = { it::class.simpleName }
                    ) { balanceRestoreState ->
                        when (balanceRestoreState) {
                            BalanceRestoreUiState.NotDetermined -> Unit

                            BalanceRestoreUiState.SendCash -> SendCashActions(
                                modifier = Modifier.fillMaxWidth(),
                                onSendClick = onSendClick,
                                onGetCashClick = onGetCashClick
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
                            onDetailsToggled = onDetailsToggled,
                            onShareLogsClick = onShareLogsClick
                        )
                    }

                    if (fundingActivity.inFlight.isNotEmpty()) {
                        VerticalSpacer { small }

                        FundingInProgressDock(
                            rows = fundingActivity.inFlight,
                            onExpand = { inProgressExpanded = true },
                            onRowClick = onFundingRowClick,
                        )
                    }

                    VerticalSpacer { large }

                    FundingHistorySection(
                        modifier = Modifier.fillMaxWidth(),
                        days = fundingActivity.days,
                        onRowClick = onFundingRowClick,
                    )
                }
            }
        }

        if (showInProgressPanel) {
            FundingInProgressPanel(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .windowInsetsPadding(LocalAppNavigationBarInsets.current)
                    .padding(PolkadotTheme.spacings.mediumIncreased),
                rows = fundingActivity.inFlight,
                onCollapse = { inProgressExpanded = false },
                onRowClick = onFundingRowClick,
            )
        }
    }
}

/** Get brings value in through the funding sheet; Send reaches contacts and, from there, moving money out. */
@Composable
private fun SendCashActions(
    modifier: Modifier = Modifier,
    onSendClick: () -> Unit,
    onGetCashClick: () -> Unit
) {
    val symbol = LocalPaymentAssetBrand.current.symbol

    Row(
        modifier = modifier.height(IntrinsicSize.Min),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
    ) {
        CashActionTile(
            modifier = Modifier.weight(1f).fillMaxHeight(),
            icon = NovaIcons.ArrowUpward,
            title = stringResource(RCommon.string.pocket_cash_get),
            caption = stringResource(RCommon.string.pocket_cash_get_caption, symbol),
            onClick = onGetCashClick
        )

        CashActionTile(
            modifier = Modifier.weight(1f).fillMaxHeight(),
            icon = NovaIcons.ArrowDownward,
            title = stringResource(RCommon.string.pocket_cash_send),
            caption = stringResource(RCommon.string.pocket_cash_send_caption, symbol),
            onClick = onSendClick
        )
    }
}

@Composable
private fun CashActionTile(
    icon: ImageVector,
    title: String,
    caption: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.containerInverted,
        onClick = onClick
    ) {
        Column(
            modifier = Modifier.padding(PolkadotTheme.spacings.medium),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall)
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall)
            ) {
                NovaIcon(modifier = Modifier.size(TILE_ICON_SIZE), imageVector = icon, tint = PolkadotTheme.colors.fg.primaryInverted)
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.title.small,
                    color = PolkadotTheme.colors.fg.primaryInverted
                )
            }
            NovaText(
                text = caption,
                style = PolkadotTheme.typography.caption.medium,
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
                        detailsVisible = false
                    )
                ),
                state = DigitalDollarCardDetailsUiState(
                    balanceRestore = BalanceRestoreUiState.SendCash
                ),
                fundingActivity = FundingActivityUiState(inFlight = persistentListOf(), days = persistentListOf()),
                onSendClick = {},
                onFundingRowClick = {},
                onGetCashClick = {},
                onAutoFundClick = {},
                onDetailsToggled = {},
                onShareLogsClick = {},
                onBackupUpdateClick = {},
                onBackupCloseClick = {}
            )
        }
    }
}
