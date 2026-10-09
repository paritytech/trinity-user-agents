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
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Clock
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingKey
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingAmountNote
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingAmountUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingPreset
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingRailTab
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCircleButton
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingKeypad
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingRailTabs
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import io.paritytech.polkadotapp.common.R as RCommon

private val PRESET_HEIGHT = 36.dp

/** The overlay's first screen: which rail, and how much. */
@Composable
fun FundingAmountScreen(
    state: FundingAmountUiState,
    onRailSelected: (FundingRail) -> Unit,
    onKey: (FundingKey) -> Unit,
    onPreset: (BigDecimal) -> Unit,
    onContinue: () -> Unit,
    onClose: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
    ) {
        FundingHeader(
            title = stringResource(if (state.isWithdraw) RCommon.string.funding_amount_withdraw_title else RCommon.string.funding_amount_add_title),
            onBack = null,
        ) {
            FundingCircleButton(icon = NovaIcons.Clock, onClick = onClose)
        }

        FundingRailTabs(tabs = state.rails, onSelect = onRailSelected)

        AmountFigure(state)

        if (state.presets.isNotEmpty()) {
            Presets(presets = state.presets, onPreset = onPreset)
        }

        FundingKeypad(modifier = Modifier.fillMaxWidth(), onKey = onKey)

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_continue),
            enabled = state.canContinue,
            onClick = onContinue,
        )
    }
}

@Composable
private fun AmountFigure(state: FundingAmountUiState) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (state.available != null) {
            NovaText(
                text = stringResource(RCommon.string.funding_amount_available, state.available),
                style = PolkadotTheme.typography.caption.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }

        NovaText(
            text = state.amount,
            style = amountStyle(state.digitCount),
            color = if (state.hasAmount) PolkadotTheme.colors.fg.primary else PolkadotTheme.colors.fg.tertiary,
            maxLines = 1,
            textAlign = TextAlign.Center,
        )

        NovaText(
            text = state.note?.let { noteText(it) } ?: " ",
            style = PolkadotTheme.typography.caption.medium,
            color = if (state.note?.isError == true) PolkadotTheme.colors.fg.error else PolkadotTheme.colors.fg.secondary,
        )
    }
}

@Composable
private fun amountStyle(digits: Int): TextStyle = when (digits) {
    in 0..3 -> PolkadotTheme.typography.display.extraLarge
    in 4..5 -> PolkadotTheme.typography.display.medium
    else -> PolkadotTheme.typography.headline.large
}

@Composable
private fun noteText(note: FundingAmountNote): String = when (note) {
    is FundingAmountNote.Minimum -> stringResource(
        if (note.withdraw) RCommon.string.funding_amount_withdraw_minimum else RCommon.string.funding_amount_minimum,
        note.amount,
    )

    is FundingAmountNote.BelowMinimum -> stringResource(RCommon.string.funding_error_minimum, note.amount)
    is FundingAmountNote.AboveMaximum -> stringResource(RCommon.string.funding_error_maximum, note.amount)
    is FundingAmountNote.NotEnough -> stringResource(RCommon.string.funding_error_not_enough, note.symbol)
}

@Composable
private fun Presets(
    presets: ImmutableList<FundingPreset>,
    onPreset: (BigDecimal) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        presets.forEach { preset ->
            PolkadotSurface(
                modifier = Modifier
                    .weight(1f)
                    .height(PRESET_HEIGHT),
                shape = PolkadotTheme.shapes.full,
                color = PolkadotTheme.colors.bg.surface.container,
                contentAlignment = Alignment.Center,
                onClick = { onPreset(preset.value) },
            ) {
                NovaText(
                    text = preset.label,
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                )
            }
        }
    }
}

@Preview
@Composable
private fun FundingAmountScreenPreview() {
    PolkadotTheme {
        FundingAmountScreen(
            state = FundingAmountUiState(
                isWithdraw = false,
                rails = persistentListOf(
                    FundingRailTab(FundingRail.CRYPTO, selected = false, enabled = true),
                    FundingRailTab(FundingRail.CARD, selected = true, enabled = true),
                    FundingRailTab(FundingRail.BANK, selected = false, enabled = false),
                ),
                amount = "$50",
                digitCount = 2,
                hasAmount = true,
                available = null,
                note = FundingAmountNote.Minimum("$10 CASH", withdraw = false),
                presets = persistentListOf(FundingPreset(BigDecimal.TEN, "$10")),
                canContinue = true,
            ),
            onRailSelected = {},
            onKey = {},
            onPreset = {},
            onContinue = {},
            onClose = {},
        )
    }
}
