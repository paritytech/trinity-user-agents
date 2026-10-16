package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain.Asset.Type
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.FundContract
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundingOperation.Status
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components.FundHeader
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components.FundingOperations
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components.FundingWidget
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.FiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.LocalFiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model.FiatAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.ConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.KnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalKnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplayId
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import kotlinx.collections.immutable.persistentListOf
import java.util.UUID
import kotlin.time.Duration.Companion.seconds

@Composable
fun FundScreen(contract: FundContract) {
    val state = contract.state.collectAsState().value

    BackHandler { contract.doneClicked() }

    when (state) {
        is LoadingState.Loading -> LoadingScreenState()
        is LoadingState.Error -> Unit
        is LoadingState.Loaded -> {
            FundScreenInternal(
                state = state.data,
                doneClick = contract::doneClicked,
                copyAddressClick = contract::copyAddressClicked
            )
        }
    }
}

@Composable
fun FundScreenInternal(
    state: FundUiState,
    doneClick: () -> Unit,
    copyAddressClick: (String) -> Unit,
) {
    PolkadotSurface {
        val tokenUiConfig = LocalKnownTokenFormatter.current.appearanceOf(state.assetDisplay)

        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .systemBarsPadding(),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            FundHeader(
                doneEnabled = state.doneEnabled,
                chainName = state.chainName,
                tokenUiConfig = tokenUiConfig,
                doneClicked = doneClick
            )

            VerticalSpacer { large }

            AnimatedVisibility(visible = state.operations.isNotEmpty()) {
                FundingOperations(
                    modifier = Modifier
                        .padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
                        .padding(bottom = PolkadotTheme.spacings.small)
                        .fillMaxWidth(),
                    operations = state.operations
                )
            }

            FundingWidget(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                minimumSendAmount = state.minimumSendAmount,
                tokenUiConfig = tokenUiConfig,
                fundingAddress = state.fundingAddress,
                copyAddressClick = copyAddressClick,
                chainName = state.chainName,
                conversion = state.conversion,
                fee = state.fee
            )
        }
    }
}

@Preview(device = "spec:width=1080px,height=3500px,dpi=440")
@Composable
private fun FundScreenInternalPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked,
            LocalConversionFormatter provides ConversionFormatter.mocked,
            LocalKnownTokenFormatter provides KnownTokenFormatter.mocked,
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current),
            LocalFiatFormatter provides FiatFormatter.mocked
        ) {
            val asset = mockAsset()
            val symbol = asset.symbol
            val address = "15oF4uVJwmo4TdGW7VfQxNLavjCXviqxT9S1MgbjMNHr6Sp5"

            FundScreenInternal(
                FundUiState(
                    true,
                    assetDisplay = AssetDisplay(
                        displayId = AssetDisplayId.USDT,
                        asset = mockAsset()
                    ),
                    chainName = "Polkadot Asset Hub",
                    minimumSendAmount = object : TokenAmountModel {
                        override val amount = 2.4.toBigDecimal()
                        override val appearance = TokenSymbolAppearance.Symbol(symbol)
                    },
                    fundingAddress = address,
                    fee = FiatAmountModel(
                        fiatAmount = 0.05.toBigDecimal(),
                        currencyDisplay = "$"
                    ),
                    conversion = ConversionModel(
                        from = object : TokenAmountModel {
                            override val amount = 1.toBigDecimal()
                            override val appearance = TokenSymbolAppearance.Symbol(symbol)
                        },
                        to = object : TokenAmountModel {
                            override val amount = 1.00081.toBigDecimal()
                            override val appearance = TokenSymbolAppearance.Symbol(symbol)
                        }
                    ),
                    operations = persistentListOf(
                        mockOperation(),
                        mockOperation().copy(status = Status.Failure),
                        mockOperation().copy(status = Status.InProgress(5.seconds)),
                    )
                ),
                doneClick = {},
                copyAddressClick = {})
        }
    }
}

fun mockAsset() = Asset(
    0, null, "", "USDT", 0, Type.Native, "Tether", true
)

fun mockOperation() = FundingOperation(
    id = UUID.randomUUID().toString(),
    status = Status.Done,
    conversion = object : TokenAmountModel {
        override val amount = 5.26.toBigDecimal()
        override val appearance = TokenSymbolAppearance.Symbol("USDT")
    } to object : TokenAmountModel {
        override val amount = 5.27145.toBigDecimal()
        override val appearance = TokenSymbolAppearance.DigitalDollar
    }
)
