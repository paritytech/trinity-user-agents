package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowRight
import io.paritytech.polkadotapp.design.components.progress.Shimmer
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingEta
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSummaryProblem
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSummaryUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCard
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingInfoRow
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingProviderLogo
import uniffi.truapi.FundingRail
import io.paritytech.polkadotapp.common.R as RCommon

private val HEADLINE_SKELETON_WIDTH = 160.dp
private val HEADLINE_SKELETON_HEIGHT = 48.dp
private val VALUE_SKELETON_WIDTH = 72.dp
private val VALUE_SKELETON_HEIGHT = 16.dp
private val ROW_LOGO_SIZE = 20.dp
private val CHEVRON_SIZE = 12.dp

/** What the chosen provider will charge and pay, before the session is handed to it. */
@Composable
fun FundingSummaryScreen(
    state: FundingSummaryUiState,
    onBack: () -> Unit,
    onFees: () -> Unit,
    onCountry: () -> Unit,
    onProviders: () -> Unit,
    onContinue: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        FundingHeader(title = stringResource(state.titleRes()), onBack = onBack)

        Column(
            modifier = Modifier.fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
        ) {
            Headline(state)
            FeesLink(state = state, onFees = onFees)
        }

        FundingCard {
            if (state.showCountry) {
                FundingInfoRow(label = stringResource(RCommon.string.funding_summary_country), onClick = onCountry) {
                    ValueText(state.country?.let { "${it.flag} ${it.name}" } ?: stringResource(RCommon.string.funding_summary_country_choose))
                }
            }

            FundingInfoRow(label = stringResource(RCommon.string.funding_summary_provider), onClick = onProviders) {
                if (state.provider != null) {
                    FundingProviderLogo(brand = state.provider, size = ROW_LOGO_SIZE)
                    ValueText(state.provider.name)
                } else {
                    ValueSkeleton()
                }
            }

            FundingInfoRow(
                label = stringResource(if (state.isWithdraw) RCommon.string.funding_fees_you_send else RCommon.string.funding_summary_min_payout),
            ) {
                if (state.payout != null) ValueText(state.payout) else ValueSkeleton()
            }

            FundingInfoRow(label = stringResource(RCommon.string.funding_summary_arrives)) {
                when {
                    state.eta != null -> ValueText(etaText(state.eta))
                    state.isQuoting -> ValueSkeleton()
                    else -> ValueText("–")
                }
            }
        }

        if (state.bankRateNoteSymbol != null) {
            NovaText(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.funding_summary_bank_rate_note, state.bankRateNoteSymbol),
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.secondary,
                textAlign = TextAlign.End,
            )
        }

        if (state.problem != null) {
            NovaText(
                modifier = Modifier.fillMaxWidth(),
                text = problemText(state.problem),
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.error,
                textAlign = TextAlign.Center,
            )
        }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_continue),
            enabled = state.canStart,
            loading = state.isStarting,
            onClick = onContinue,
        )
    }
}

@Composable
private fun Headline(state: FundingSummaryUiState) {
    when {
        state.headline != null -> NovaText(
            text = state.headline,
            style = PolkadotTheme.typography.display.extraLarge,
            color = PolkadotTheme.colors.fg.primary,
            maxLines = 1,
        )

        state.isQuoting -> Shimmer(
            modifier = Modifier
                .width(HEADLINE_SKELETON_WIDTH)
                .height(HEADLINE_SKELETON_HEIGHT),
        )

        else -> NovaText(
            text = "–",
            style = PolkadotTheme.typography.display.extraLarge,
            color = PolkadotTheme.colors.fg.tertiary,
        )
    }
}

@Composable
private fun FeesLink(state: FundingSummaryUiState, onFees: () -> Unit) {
    Row(
        modifier = Modifier.clickable(enabled = state.headline != null, onClick = onFees),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaText(
            text = stringResource(state.feesCaptionRes()),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary,
        )
        HorizontalSpacer { extraSmall }
        NovaIcon(
            modifier = Modifier.size(CHEVRON_SIZE),
            imageVector = NovaIcons.ArrowRight,
            tint = PolkadotTheme.colors.fg.secondary,
        )
    }
}

@Composable
private fun ValueText(text: String) {
    NovaText(
        text = text,
        style = PolkadotTheme.typography.body.mediumEmphasized,
        color = PolkadotTheme.colors.fg.primary,
        maxLines = 1,
    )
}

@Composable
private fun ValueSkeleton() {
    Shimmer(
        modifier = Modifier
            .width(VALUE_SKELETON_WIDTH)
            .height(VALUE_SKELETON_HEIGHT),
    )
}

@Composable
private fun etaText(eta: FundingEta): String = when (eta) {
    is FundingEta.Minutes -> stringResource(RCommon.string.funding_eta_minutes, eta.count)
    is FundingEta.Hours -> stringResource(RCommon.string.funding_eta_hours, eta.count)
    is FundingEta.Days -> stringResource(RCommon.string.funding_eta_days, eta.count)
}

@Composable
private fun problemText(problem: FundingSummaryProblem): String = when (problem) {
    FundingSummaryProblem.StartFailed -> stringResource(RCommon.string.funding_error_start_failed)
    FundingSummaryProblem.NoProvider -> stringResource(RCommon.string.funding_error_no_provider)
    is FundingSummaryProblem.Minimum -> stringResource(RCommon.string.funding_error_minimum, problem.amount)
    is FundingSummaryProblem.Maximum -> stringResource(RCommon.string.funding_error_maximum, problem.amount)
}

private fun FundingSummaryUiState.titleRes(): Int = when (rail) {
    FundingRail.CARD -> if (isWithdraw) RCommon.string.funding_summary_withdraw_card_title else RCommon.string.funding_summary_card_title
    FundingRail.BANK -> if (isWithdraw) RCommon.string.funding_summary_withdraw_bank_title else RCommon.string.funding_summary_bank_title
    FundingRail.CRYPTO -> if (isWithdraw) RCommon.string.funding_summary_withdraw_crypto_title else RCommon.string.funding_summary_crypto_title
}

private fun FundingSummaryUiState.feesCaptionRes(): Int = when {
    isWithdraw -> RCommon.string.funding_summary_withdraw_caption
    rail == FundingRail.CARD -> RCommon.string.funding_summary_card_caption
    else -> RCommon.string.funding_summary_bank_caption
}
