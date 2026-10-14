package io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Dollar
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.common.SettingsSelectionDivider
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.CurrencyContract
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.compose.components.CurrencyItem
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.models.CurrencyState
import io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.models.CurrencyUiModel

@Composable
fun CurrencyScreen(contract: CurrencyContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    CurrencyScreenInternal(
        state = state,
        onCurrencySelected = contract::onCurrencySelected,
        onBackClick = contract::onBackClick
    )
}

@Composable
private fun CurrencyScreenInternal(
    state: CurrencyState,
    onCurrencySelected: (CurrencyUiModel) -> Unit,
    onBackClick: () -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(id = R.string.settings_currency),
                navigationAction = rememberTopBarAction(onBackClick),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            LazyColumn(
                modifier = Modifier.fillMaxSize()
            ) {
                itemsIndexed(state.availableCurrencies) { index, currency ->
                    CurrencyItem(
                        title = currency.name,
                        subtitle = currency.code,
                        currencyIcon = currency.icon,
                        isSelected = currency.code == state.selectedCurrency?.code,
                        onClick = { onCurrencySelected(currency) }
                    )

                    if (index < state.availableCurrencies.lastIndex) {
                        SettingsSelectionDivider()
                    }
                }
            }
        }
    }
}

@Preview
@Composable
private fun CurrencyScreenPreview() {
    val usd = CurrencyUiModel(
        code = "USD",
        name = "US Dollar",
        icon = NovaIcons.Dollar,
    )

    PolkadotTheme {
        CurrencyScreenInternal(
            state = CurrencyState(
                selectedCurrency = usd,
                availableCurrencies = listOf(usd)
            ),
            onCurrencySelected = {},
            onBackClick = {}
        )
    }
}
