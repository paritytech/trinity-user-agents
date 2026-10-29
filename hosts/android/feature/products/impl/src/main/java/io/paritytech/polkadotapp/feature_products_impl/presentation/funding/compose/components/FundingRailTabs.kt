package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingRailTab
import kotlinx.collections.immutable.ImmutableList
import uniffi.truapi.FundingRail
import io.paritytech.polkadotapp.common.R as RCommon

private val PILL_HEIGHT = 32.dp
private const val DISABLED_ALPHA = 0.4f

/** Crypto, Card and Bank as pills. A rail no provider serves for this direction is shown but cannot be picked. */
@Composable
fun FundingRailTabs(
    tabs: ImmutableList<FundingRailTab>,
    onSelect: (FundingRail) -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        tabs.forEach { tab ->
            PolkadotSurface(
                modifier = Modifier
                    .height(PILL_HEIGHT)
                    .alpha(if (tab.enabled) 1f else DISABLED_ALPHA),
                shape = PolkadotTheme.shapes.full,
                color = if (tab.selected) PolkadotTheme.colors.bg.surface.containerInverted else PolkadotTheme.colors.bg.surface.container,
                contentAlignment = Alignment.Center,
                enabled = tab.enabled,
                onClick = { onSelect(tab.rail) },
            ) {
                NovaText(
                    modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraMedium),
                    text = stringResource(tab.rail.titleRes()),
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = if (tab.selected) PolkadotTheme.colors.fg.primaryInverted else PolkadotTheme.colors.fg.primary,
                )
            }
        }
    }
}

fun FundingRail.titleRes(): Int = when (this) {
    FundingRail.CRYPTO -> RCommon.string.funding_rail_crypto
    FundingRail.CARD -> RCommon.string.funding_rail_card
    FundingRail.BANK -> RCommon.string.funding_rail_bank
}
