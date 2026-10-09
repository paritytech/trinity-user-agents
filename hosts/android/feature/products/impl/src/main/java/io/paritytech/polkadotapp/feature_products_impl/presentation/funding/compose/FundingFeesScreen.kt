package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFeesUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingPayTitle
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.common.R as RCommon

private val DIVIDER_HEIGHT = 1.dp

/** How the chosen quote's charge breaks down. */
@Composable
fun FundingFeesScreen(
    state: FundingFeesUiState?,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        FundingHeader(title = stringResource(RCommon.string.funding_fees_title), onBack = onBack)

        if (state != null) {
            Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                FeeLine(stringResource(RCommon.string.funding_fees_provider), state.providerFee)
                FeeLine(stringResource(RCommon.string.funding_fees_network), state.networkFee)
                FeeLine(stringResource(RCommon.string.funding_fees_swapping), stringResource(RCommon.string.funding_fees_variable))
                Divider()
                FeeLine(stringResource(RCommon.string.funding_fees_total), state.totalFee)
                Divider()

                Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    NovaText(
                        modifier = Modifier.weight(1f),
                        text = stringResource(state.payTitle.titleRes()),
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.secondary,
                    )
                    NovaText(
                        text = state.pay,
                        style = PolkadotTheme.typography.headline.small,
                        color = PolkadotTheme.colors.fg.primary,
                    )
                }

                if (state.rate != null) {
                    FeeLine(
                        title = stringResource(RCommon.string.funding_fees_rate),
                        value = stringResource(RCommon.string.funding_fees_rate_value, state.rate.symbol, state.rate.perCash),
                        emphasized = false,
                    )
                }
            }
        }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_back),
            style = PolkadotButtonStyle.secondary(),
            onClick = onBack,
        )
    }
}

@Composable
private fun FeeLine(title: String, value: String, emphasized: Boolean = true) {
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
        )
        NovaText(
            text = value,
            style = if (emphasized) PolkadotTheme.typography.body.mediumEmphasized else PolkadotTheme.typography.body.large,
            color = if (emphasized) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.secondary,
        )
    }
}

@Composable
private fun Divider() {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .height(DIVIDER_HEIGHT),
        color = PolkadotTheme.colors.stroke.primary,
    ) {}
}

private fun FundingPayTitle.titleRes(): Int = when (this) {
    FundingPayTitle.YOU_PAY -> RCommon.string.funding_fees_you_pay
    FundingPayTitle.AMOUNT_TO_SEND -> RCommon.string.funding_fees_amount_to_send
    FundingPayTitle.YOU_SEND -> RCommon.string.funding_fees_you_send
}
