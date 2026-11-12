package io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.compose

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.KnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalKnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplayId
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.SimpleAssetListContract
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.compose.components.AssetItem
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models.SimpleAssetListUiState
import io.paritytech.polkadotapp.feature_tokens_api.presentation.simpletokenlist.models.SimpleTokenListUiConfig
import kotlinx.collections.immutable.persistentListOf

@Composable
fun SimpleAssetListScreen(contract: SimpleAssetListContract, config: SimpleTokenListUiConfig) {
    val state by contract.state.collectAsState()

    SimpleAssetListScreenInternal(
        state = state,
        config = config,
        onBackClick = contract::backClicked,
        onAssetClick = contract::tokenClicked
    )
}

@Composable
private fun SimpleAssetListScreenInternal(
    state: SimpleAssetListUiState,
    config: SimpleTokenListUiConfig,
    onBackClick: () -> Unit,
    onAssetClick: (Asset) -> Unit,
) {
    val formatter = LocalKnownTokenFormatter.current
    PolkadotSurface {
        Column(
            modifier = Modifier
                .safeDrawingPadding()
                .fillMaxSize()
        ) {
            PolkadotTopBar(
                navigationAction = rememberTopBarAction(
                    action = onBackClick,
                    icon = NovaIcons.Close
                ),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            VerticalSpacer { small }

            NovaText(
                modifier = Modifier
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                    .fillMaxWidth(),
                text = stringResource(config.titleRes, *config.titleArgs.toTypedArray())
                    .withCurrencyTickerStyle(PolkadotTheme.typography.headline.large),
                style = PolkadotTheme.typography.headline.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center
            )

            VerticalSpacer { 62.dp }

            LazyColumn(
                modifier = Modifier.fillMaxSize()
            ) {
                items(
                    items = state.assets,
                    key = { it.displayId.toString() }
                ) { item ->
                    AssetItem(
                        uiConfig = formatter.appearanceOf(item),
                        onClick = { onAssetClick(item.asset) }
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun WalletScreenPreview() {
    CompositionLocalProvider(
        LocalKnownTokenFormatter provides KnownTokenFormatter.mocked,
        LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
    ) {
        PolkadotTheme {
            SimpleAssetListScreenInternal(
                state = SimpleAssetListUiState(
                    assets = listOf(
                        AssetDisplay(
                            displayId = AssetDisplayId.DOT,
                            asset = mockAsset()
                        ),
                        AssetDisplay(
                            displayId = AssetDisplayId.USDT,
                            asset = mockAsset().copy(id = 1, symbol = "USDT", name = "Tether")
                        ),
                        AssetDisplay(
                            displayId = AssetDisplayId.USDC,
                            asset = mockAsset().copy(id = 2, symbol = "USDC", name = "Circle")
                        )
                    )
                ),
                config = SimpleTokenListUiConfig(R.string.asset_details_fund_button, persistentListOf("DOT")),
                onBackClick = {},
                onAssetClick = {},
            )
        }
    }
}

private fun mockAsset() = Asset(
    0, null, "", "DOT", 0, Type.Native, "Polkadot", true
)
