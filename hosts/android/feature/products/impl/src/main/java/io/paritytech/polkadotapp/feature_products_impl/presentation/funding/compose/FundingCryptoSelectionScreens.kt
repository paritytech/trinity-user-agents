package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingNetworkRow
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTokensUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingMonogram
import kotlinx.collections.immutable.ImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

/** The networks the providers' crypto routes name. */
@Composable
fun FundingNetworkScreen(
    rows: ImmutableList<FundingNetworkRow>,
    onBack: () -> Unit,
    onNetwork: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(title = stringResource(RCommon.string.funding_network_title), onBack = onBack)

        if (rows.isEmpty()) {
            NovaText(
                modifier = Modifier.padding(top = PolkadotTheme.spacings.large),
                text = stringResource(RCommon.string.funding_error_no_provider),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary,
            )
        }

        LazyColumn(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
            items(rows, key = { it.id }) { row ->
                FundingSelectionRow(
                    monogram = row.monogram,
                    title = row.name,
                    onClick = { onNetwork(row.id) },
                )
            }
        }
    }
}

/** The tokens the providers take on the chosen network. */
@Composable
fun FundingTokenScreen(
    state: FundingTokensUiState,
    onBack: () -> Unit,
    onToken: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(title = stringResource(RCommon.string.funding_token_title), onBack = onBack)

        if (state.networkName != null) {
            PolkadotSurface(
                modifier = Modifier.fillMaxWidth(),
                shape = PolkadotTheme.shapes.full,
                color = PolkadotTheme.colors.bg.surface.container,
            ) {
                NovaText(
                    modifier = Modifier.padding(
                        horizontal = PolkadotTheme.spacings.smallIncreased,
                        vertical = PolkadotTheme.spacings.small,
                    ),
                    text = stringResource(RCommon.string.funding_token_network_selected, state.networkName),
                    style = PolkadotTheme.typography.caption.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                )
            }
        }

        LazyColumn(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
            items(state.rows, key = { it.symbol }) { row ->
                FundingSelectionRow(
                    monogram = row.monogram,
                    title = row.symbol,
                    onClick = { onToken(row.symbol) },
                )
            }
        }
    }
}

@Composable
private fun FundingSelectionRow(
    monogram: String,
    title: String,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.medium),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
        ) {
            FundingMonogram(text = monogram)
            NovaText(
                text = title,
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
            )
        }
    }
}
