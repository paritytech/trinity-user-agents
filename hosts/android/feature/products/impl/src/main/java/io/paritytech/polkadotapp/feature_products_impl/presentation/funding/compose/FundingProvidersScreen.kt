package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.progress.Shimmer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCountdown
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingProviderBadge
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingProviderPrice
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingProviderRow
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingProvidersUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingProviderLogo
import java.util.Locale
import io.paritytech.polkadotapp.common.R as RCommon

private val LOGO_SIZE = 36.dp
private val PRICE_SKELETON_WIDTH = 64.dp
private val CAPTION_SKELETON_WIDTH = 48.dp
private val SKELETON_HEIGHT = 12.dp
private const val DIMMED_ALPHA = 0.5f
private const val WARNING_SECONDS = 20L
private const val ERROR_SECONDS = 10L
private const val SECONDS_PER_MINUTE = 60

/** Every provider's live quote, the cheapest marked, with how long the prices hold before they are asked again. */
@Composable
fun FundingProvidersScreen(
    state: FundingProvidersUiState,
    onBack: () -> Unit,
    onProvider: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(title = stringResource(RCommon.string.funding_providers_title), onBack = onBack)

        Countdown(state.countdown)

        Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall)) {
            state.rows.forEach { row -> ProviderRow(row = row, onClick = { onProvider(row.brand.providerId) }) }
        }
    }
}

@Composable
private fun Countdown(countdown: FundingCountdown?) {
    when (countdown) {
        FundingCountdown.Pending -> Shimmer(
            modifier = Modifier
                .width(PRICE_SKELETON_WIDTH * 2)
                .height(SKELETON_HEIGHT),
        )

        is FundingCountdown.Remaining -> {
            val seconds = countdown.seconds
            NovaText(
                text = stringResource(
                    RCommon.string.funding_providers_rate_updated_in,
                    String.format(Locale.ROOT, "%d:%02d", seconds / SECONDS_PER_MINUTE, seconds % SECONDS_PER_MINUTE),
                ),
                style = PolkadotTheme.typography.body.smallEmphasized,
                color = when {
                    seconds <= ERROR_SECONDS -> PolkadotTheme.colors.fg.error
                    seconds <= WARNING_SECONDS -> PolkadotTheme.colors.fg.warning
                    else -> PolkadotTheme.colors.fg.secondary
                },
            )
        }

        null -> Unit
    }
}

@Composable
private fun ProviderRow(row: FundingProviderRow, onClick: () -> Unit) {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .alpha(if (row.dimmed) DIMMED_ALPHA else 1f),
        shape = PolkadotTheme.shapes.large,
        color = if (row.selected) PolkadotTheme.colors.bg.surface.nested else PolkadotTheme.colors.bg.surface.main,
        enabled = row.enabled,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
        ) {
            FundingProviderLogo(brand = row.brand, size = LOGO_SIZE)

            Column(modifier = Modifier.weight(1f)) {
                NovaText(
                    text = row.brand.name,
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                )
                if (row.badge != null) {
                    NovaText(
                        text = badgeText(row.badge),
                        style = PolkadotTheme.typography.caption.medium,
                        color = if (row.badge == FundingProviderBadge.LowestPrice) PolkadotTheme.colors.fg.success else PolkadotTheme.colors.fg.secondary,
                    )
                }
            }

            Price(row.price)
        }
    }
}

@Composable
private fun Price(price: FundingProviderPrice?) {
    Column(
        horizontalAlignment = Alignment.End,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraSmall),
    ) {
        when (price) {
            FundingProviderPrice.Pending -> {
                Shimmer(modifier = Modifier.width(PRICE_SKELETON_WIDTH).height(SKELETON_HEIGHT))
                Shimmer(modifier = Modifier.width(CAPTION_SKELETON_WIDTH).height(SKELETON_HEIGHT))
            }

            is FundingProviderPrice.Quoted -> {
                NovaText(
                    text = "≈ ${price.price}",
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                )
                NovaText(
                    text = stringResource(RCommon.string.funding_providers_for, price.forAmount),
                    style = PolkadotTheme.typography.caption.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                )
            }

            null -> Unit
        }
    }
}

@Composable
private fun badgeText(badge: FundingProviderBadge): String = when (badge) {
    FundingProviderBadge.LowestPrice -> stringResource(RCommon.string.funding_providers_lowest_price)
    FundingProviderBadge.Unavailable -> stringResource(RCommon.string.funding_providers_unavailable)
    FundingProviderBadge.CountryUnsupported -> stringResource(RCommon.string.funding_providers_country_unsupported)
    is FundingProviderBadge.Minimum -> stringResource(RCommon.string.funding_error_minimum, badge.amount)
    is FundingProviderBadge.Maximum -> stringResource(RCommon.string.funding_error_maximum, badge.amount)
}
