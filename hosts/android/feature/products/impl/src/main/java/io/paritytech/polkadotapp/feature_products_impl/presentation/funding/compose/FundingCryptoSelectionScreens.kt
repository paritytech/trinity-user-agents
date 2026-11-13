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
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCryptoIcon
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingNetworkRow
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTokensUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingCryptoIconImage
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import kotlinx.collections.immutable.ImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

private const val DISABLED_ALPHA = 0.5f
private val CHIP_ICON_SIZE = 16.dp

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
                    icon = row.icon,
                    title = row.name,
                    subtitle = row.minimum?.let { stringResource(RCommon.string.funding_network_minimum, it) },
                    enabled = row.minimum == null,
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
                Row(
                    modifier = Modifier.padding(
                        horizontal = PolkadotTheme.spacings.smallIncreased,
                        vertical = PolkadotTheme.spacings.small,
                    ),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
                ) {
                    state.networkIcon?.takeIf { it !is FundingCryptoIcon.Monogram }?.let {
                        FundingCryptoIconImage(icon = it, size = CHIP_ICON_SIZE)
                    }
                    NovaText(
                        text = stringResource(RCommon.string.funding_token_network_selected, state.networkName),
                        style = PolkadotTheme.typography.caption.medium,
                        color = PolkadotTheme.colors.fg.secondary,
                    )
                }
            }
        }

        LazyColumn(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
            items(state.rows, key = { it.symbol }) { row ->
                FundingSelectionRow(
                    icon = row.icon,
                    title = row.symbol,
                    subtitle = null,
                    enabled = true,
                    onClick = { onToken(row.symbol) },
                )
            }
        }
    }
}

@Composable
private fun FundingSelectionRow(
    icon: FundingCryptoIcon,
    title: String,
    subtitle: String?,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(if (enabled) 1f else DISABLED_ALPHA),
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container,
        enabled = enabled,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.medium),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
        ) {
            FundingCryptoIconImage(icon = icon)
            Column {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                )
                if (subtitle != null) {
                    NovaText(
                        text = subtitle,
                        style = PolkadotTheme.typography.body.small,
                        color = PolkadotTheme.colors.fg.secondary,
                    )
                }
            }
        }
    }
}
