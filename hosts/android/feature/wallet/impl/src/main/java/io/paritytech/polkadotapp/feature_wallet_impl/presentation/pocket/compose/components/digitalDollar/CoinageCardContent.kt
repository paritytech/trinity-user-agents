package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageBalanceBreakdownUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.CoinageUiState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CoinageCardContent(
    state: CoinageUiState,
    onAutoFundClick: () -> Unit,
    onDetailsToggled: () -> Unit,
    onShareLogsClick: () -> Unit
) {
    Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)) {
        CoinageStateCard(
            modifier = Modifier.fillMaxWidth(),
            state = state.tokensState,
            detailsVisible = state.detailsVisible,
            onDetailsToggled = onDetailsToggled
        )

        if (FeatureOption.COINAGE_DEBUG_FEATURES.isEnabled) {
            DebugFeaturesCard(
                modifier = Modifier.fillMaxWidth(),
                state = state,
                onAutoFundClick = onAutoFundClick,
                onShareLogsClick = onShareLogsClick
            )
        }
    }
}

@Composable
private fun DebugFeaturesCard(
    modifier: Modifier = Modifier,
    state: CoinageUiState,
    onAutoFundClick: () -> Unit,
    onShareLogsClick: () -> Unit
) {
    CoinageWidgetCard(
        modifier = modifier,
        title = stringResource(RCommon.string.pocket_debug_features_title),
        subtitle = stringResource(RCommon.string.pocket_debug_features_subtitle)
    ) {
        BalanceBreakdownTable(breakdown = state.tokensState.breakdown)

        if (state.autoFundAvailable) {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.pocket_digital_dollar_faucet_top_up),
                style = PolkadotButtonStyle.secondary(),
                enabled = state.actionsEnabled,
                loading = state.fundInProgress,
                onClick = onAutoFundClick
            )
        }

        if (state.shareLogsEnabled) {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.pocket_coinage_share_logs),
                style = PolkadotButtonStyle.secondary(),
                onClick = onShareLogsClick
            )
        }
    }
}

@Composable
private fun BalanceBreakdownTable(breakdown: CoinageBalanceBreakdownUiModel) {
    val formatter = LocalTokenAmountFormatter.current

    Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)) {
        listOf(
            RCommon.string.pocket_debug_breakdown_private to breakdown.availablePrivate,
            RCommon.string.pocket_debug_breakdown_gaining_privacy to breakdown.gainingPrivacy,
            RCommon.string.pocket_debug_breakdown_pending to breakdown.pending
        ).forEach { (label, amount) ->
            BreakdownRow(
                label = stringResource(label),
                value = formatter.formatTokenAmount(amount, RoundPrecision.HIGH, withSymbol = false)
            )
        }

        BreakdownRow(
            label = stringResource(RCommon.string.pocket_debug_breakdown_can_spend_gaining),
            value = breakdown.canSpendGainingPrivacy.toString()
        )
    }
}

@Composable
private fun BreakdownRow(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween
    ) {
        NovaText(
            text = label,
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary
        )

        NovaText(
            text = value,
            maxLines = 1,
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.primary
        )
    }
}
