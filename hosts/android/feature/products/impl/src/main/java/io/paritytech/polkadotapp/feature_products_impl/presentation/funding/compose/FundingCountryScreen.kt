package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotSearchField
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCountryUi
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCountryUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.compose.components.FundingHeader
import io.paritytech.polkadotapp.common.R as RCommon

/** Where the user pays from. Countries every provider on this rail refuses are listed apart and cannot be picked. */
@Composable
fun FundingCountryScreen(
    state: FundingCountryUiState,
    onBack: () -> Unit,
    onQueryChanged: (String) -> Unit,
    onCountry: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium),
    ) {
        FundingHeader(title = stringResource(RCommon.string.funding_country_title), onBack = onBack)

        PolkadotSearchField(
            modifier = Modifier.fillMaxWidth(),
            value = state.query,
            onValueChange = onQueryChanged,
            onClear = { onQueryChanged("") },
            placeholder = stringResource(RCommon.string.funding_country_search),
        )

        LazyColumn(modifier = Modifier.fillMaxWidth()) {
            if (state.detected != null) {
                sectionTitle(RCommon.string.funding_country_detected)
                item(key = "detected") { CountryRow(country = state.detected, onClick = onCountry) }
                sectionTitle(RCommon.string.funding_country_other)
            }

            items(state.supported, key = { it.code }) { CountryRow(country = it, onClick = onCountry) }

            if (state.unsupported.isNotEmpty()) {
                sectionTitle(RCommon.string.funding_country_unsupported)
                items(state.unsupported, key = { "unsupported-${it.code}" }) { CountryRow(country = it, onClick = onCountry) }
            }
        }
    }
}

private fun LazyListScope.sectionTitle(titleRes: Int) {
    item(key = "section-$titleRes") {
        NovaText(
            modifier = Modifier.padding(top = PolkadotTheme.spacings.medium, bottom = PolkadotTheme.spacings.extraSmall),
            text = stringResource(titleRes),
            style = PolkadotTheme.typography.body.small,
            color = PolkadotTheme.colors.fg.secondary,
        )
    }
}

@Composable
private fun CountryRow(country: FundingCountryUi, onClick: (String) -> Unit) {
    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.large,
        color = if (country.selected) PolkadotTheme.colors.bg.surface.nested else PolkadotTheme.colors.bg.surface.main,
        enabled = !country.refused,
        onClick = { onClick(country.code) },
    ) {
        Row(
            modifier = Modifier.padding(PolkadotTheme.spacings.small),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.smallIncreased),
        ) {
            NovaText(text = country.flag, style = PolkadotTheme.typography.emoji.large)
            Column {
                NovaText(
                    text = country.name,
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = if (country.refused) PolkadotTheme.colors.fg.tertiary else PolkadotTheme.colors.fg.primary,
                )
                val subtitle = if (country.refused) stringResource(RCommon.string.funding_country_refused) else country.currencyName
                if (subtitle != null) {
                    NovaText(
                        text = subtitle,
                        style = PolkadotTheme.typography.body.small,
                        color = if (country.refused) PolkadotTheme.colors.fg.tertiary else PolkadotTheme.colors.fg.secondary,
                    )
                }
            }
        }
    }
}
