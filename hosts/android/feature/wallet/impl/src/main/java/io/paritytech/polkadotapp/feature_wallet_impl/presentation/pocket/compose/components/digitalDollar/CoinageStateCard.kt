package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.compose.TickerWordmark
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiatSigned
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageCoins
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageCoinsMetrics
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageRun
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageRunMarkers
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageStripLayout
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.holdings.HoldingGeometry
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageBalanceBreakdownUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import kotlinx.collections.immutable.persistentListOf
import kotlinx.collections.immutable.persistentMapOf
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * The total, and every holding that makes it up drawn as a coin.
 *
 * Collapsed, the coins are a strip of fixed height whatever the count, with a rule under each run saying
 * which bucket it is and what it comes to. A tap or a flick down on them spreads the same coins into a
 * honeycomb, one block per bucket under its own header, and the rules go away with the runs they ruled. There
 * is no label saying so, by request: the coins are their own control.
 *
 * Spread out, a tap on a coin picks it out, large, with what it is beside it, and a tap on it or on empty
 * ground puts it back. The grid closes from the card's own heading, never from a tap among the coins.
 *
 * There is no bar, no legend and no list. The coins are the depiction: they are already grouped into the two
 * buckets, so a bar above them said the same thing twice, and a row per holding said it a third time.
 *
 * "Ready" and "Clearing" are deliberately not the pallet's words: "recycler" and its relatives stay in code
 * identifiers and never reach the screen. Clearing folds together everything that is not ready yet, whatever
 * the reason, because to the user those reasons look alike.
 */
@Composable
internal fun CoinageStateCard(
    modifier: Modifier = Modifier,
    state: CoinageUiState.TokensState,
    detailsVisible: Boolean,
    focusedCoin: String?,
    onCoinTapped: (String?) -> Unit,
    onCoinsFlickedDown: () -> Unit,
    onHeadingClicked: () -> Unit
) {
    var metrics by remember { mutableStateOf(CoinageCoinsMetrics()) }
    // A coin that has since left the holdings, or has nothing to say, is no longer picked.
    val picked = focusedCoin?.takeIf { id -> id in state.traces && state.coins.any { it.id == id } }
    val headingClick = if (detailsVisible) onHeadingClicked else null

    CoinageWidgetCard(
        modifier = modifier,
        title = stringResource(RCommon.string.pocket_coinage_balance_title),
        onHeaderClick = headingClick
    ) {
        Headline(
            modifier = Modifier.headingClick(headingClick),
            total = state.totalBalance
        )

        // Nothing held draws nothing: no coins, no band of reserved height where they would be, no rules, and
        // no toggle into an empty grid. The heading and the total are the whole card. There is no partial
        // version of this to handle — the figures and the holdings come off one snapshot, so the card can
        // never have a balance whose coins are missing or coins whose balance is.
        if (state.coins.isNotEmpty()) {
            Box(modifier = Modifier.fillMaxWidth()) {
                CoinageCoins(
                    coins = state.coins,
                    isExpanded = detailsVisible,
                    focusedCoin = picked,
                    onMetrics = { metrics = it },
                    onCoinTapped = onCoinTapped,
                    onFlickedDown = onCoinsFlickedDown,
                    detail = { id ->
                        state.traces[id]?.let { trace -> CoinageTraceDetail(trace = trace) }
                    }
                )

                BlockHeaders(metrics = metrics)
            }

            // Only while the coins are in the strip: spread into the grid they have headers of their own, and
            // the runs these rule no longer exist.
            AnimatedVisibility(visible = !detailsVisible, enter = fadeIn(), exit = fadeOut()) {
                CoinageRunMarkers(runs = state.runs(metrics))
            }
        }
    }
}

@Composable
private fun Headline(
    modifier: Modifier = Modifier,
    total: TokenAmountModel
) {
    val formatter = LocalTokenAmountFormatter.current

    Row(modifier = modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(HoldingGeometry.headlineSpacing)) {
        NovaText(
            modifier = Modifier.alignByBaseline(),
            text = formatter.formatFiatSigned(total),
            maxLines = 1,
            style = PolkadotTheme.typography.headline.large,
            color = PolkadotTheme.colors.fg.primary
        )
        NovaText(
            modifier = Modifier.alignByBaseline(),
            text = LocalPaymentAssetBrand.current.symbol
                .withCurrencyTickerStyle(PolkadotTheme.typography.headline.large, TickerWordmark.SmallCaps),
            style = PolkadotTheme.typography.headline.large,
            color = PolkadotTheme.colors.fg.secondary
        )
    }
}

/**
 * The Clearing and Ready headers over the grid. The layout leaves room for them above each block, so they
 * sit in space the coins already made rather than pushing them about.
 */
@Composable
private fun BlockHeaders(metrics: CoinageCoinsMetrics) {
    metrics.blocks.forEach { block ->
        NovaText(
            modifier = Modifier.offset(y = block.top.dp),
            text = stringResource(block.partition.label),
            maxLines = 1,
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary
        )
    }
}

@Composable
private fun CoinageUiState.TokensState.runs(metrics: CoinageCoinsMetrics) =
    metrics.runs.map { run ->
        val formatter = LocalTokenAmountFormatter.current

        CoinageRun(
            partition = run.partition,
            start = run.start,
            end = run.end,
            title = stringResource(run.partition.label),
            amount = formatter.formatFiatSigned(
                when (run.partition) {
                    CoinageStripLayout.Partition.READY -> readyBalance
                    CoinageStripLayout.Partition.CLEARING -> clearingBalance
                }
            )
        )
    }.toImmutableList()

internal val CoinageStripLayout.Partition.label: Int
    get() = when (this) {
        CoinageStripLayout.Partition.READY -> RCommon.string.pocket_coinage_ready
        CoinageStripLayout.Partition.CLEARING -> RCommon.string.pocket_coinage_clearing
    }

/** No ripple: the heading is not a button, it only answers a tap while the coins are spread out. */
internal fun Modifier.headingClick(onClick: (() -> Unit)?): Modifier = if (onClick == null) {
    this
} else {
    clickable(interactionSource = null, indication = null, onClick = onClick)
}

@Preview
@Composable
private fun CoinageStateCardPreview() {
    CompositionLocalProvider(
        LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
        LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
    ) {
        PolkadotTheme {
            CoinageStateCard(
                modifier = Modifier.fillMaxWidth(),
                state = CoinageUiState.TokensState(
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
                detailsVisible = false,
                focusedCoin = null,
                onCoinTapped = {},
                onCoinsFlickedDown = {},
                onHeadingClicked = {}
            )
        }
    }
}
