package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCancelUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import uniffi.truapi.FundingRail
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant
import io.paritytech.polkadotapp.common.R as RCommon

/** The provider's terms for cancelling: what it keeps and what comes back, for the user to accept or decline. */
@Composable
fun FundingCancelOfferScreen(
    modifier: Modifier = Modifier,
    offer: FundingCancelUiState.Offered,
    withdraw: Boolean,
    isAnswering: Boolean,
    now: Instant,
    onBack: () -> Unit,
    onAccept: () -> Unit,
    onDecline: () -> Unit,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(modifier = Modifier.fillMaxWidth(), title = "", onBack = onBack)
        NovaText(
            text = stringResource(
                if (withdraw) RCommon.string.funding_cancel_offer_title_withdrawal else RCommon.string.funding_cancel_offer_title,
            ),
            style = PolkadotTheme.typography.headline.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
        NovaText(
            text = stringResource(RCommon.string.funding_cancel_offer_body),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )
        Column(modifier = Modifier.fillMaxWidth()) {
            OfferRow(stringResource(RCommon.string.funding_cancel_offer_fee), offer.fee)
            OfferRow(stringResource(RCommon.string.funding_cancel_offer_refund), offer.refund)
        }
        val secondsLeft = offer.secondsLeft(now)
        val lapsed = secondsLeft == 0L
        secondsLeft?.let { seconds ->
            NovaText(
                text = if (lapsed) {
                    stringResource(RCommon.string.funding_cancel_offer_expired)
                } else {
                    stringResource(RCommon.string.funding_cancel_offer_valid_for, LocalTimeFormatter.current.formatCountdown(seconds.seconds))
                },
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.funding_cancel_offer_accept),
            style = PolkadotButtonStyle.destructive(),
            loading = isAnswering,
            enabled = !lapsed,
            onClick = onAccept,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.funding_cancel_offer_keep),
            enabled = !isAnswering,
            onClick = onDecline,
        )
    }
}

@Composable
private fun OfferRow(title: String, value: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = PolkadotTheme.spacings.small),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )
        NovaText(text = value, style = PolkadotTheme.typography.body.mediumEmphasized, color = PolkadotTheme.colors.fg.primary)
    }
}

/**
 * Asks before cancelling. A crypto top-up's deposit address stops working; once money has moved, the provider decides
 * whether it can still cancel, and may charge a fee.
 */
@Composable
fun FundingCancelConfirmScreen(
    modifier: Modifier = Modifier,
    state: FundingDepositUiState,
    onBack: () -> Unit,
    onConfirm: () -> Unit,
) {
    Column(
        modifier = modifier,
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(modifier = Modifier.fillMaxWidth(), title = "", onBack = onBack)
        NovaText(
            text = stringResource(if (state.withdraw) RCommon.string.funding_cancel_withdrawal_title else RCommon.string.funding_cancel_title),
            style = PolkadotTheme.typography.headline.medium,
            color = PolkadotTheme.colors.fg.primary,
        )
        NovaText(
            text = stringResource(
                when {
                    state.content is FundingDepositContent.Crypto -> RCommon.string.funding_cancel_body
                    state.rail == FundingRail.BANK && !state.moneyMoved -> RCommon.string.funding_cancel_body_bank
                    else -> RCommon.string.funding_cancel_body_provider
                },
            ),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_cancel),
            style = PolkadotButtonStyle.destructive(),
            loading = state.isCancelling,
            onClick = onConfirm,
        )
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.funding_cancel_keep),
            onClick = onBack,
        )
    }
}
