package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.icon.vectors.Failure
import io.paritytech.polkadotapp.design.components.qr.QrCode
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.ConversionModel
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.mockAsset
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.FiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.LocalFiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model.FiatAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.ConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.KnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalConversionFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalKnownTokenFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.KnownTokenUiConfig
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance

@Composable
fun FundingWidget(
    modifier: Modifier = Modifier,
    minimumSendAmount: TokenAmountModel,
    tokenUiConfig: KnownTokenUiConfig,
    fundingAddress: String,
    copyAddressClick: (String) -> Unit,
    chainName: String,
    conversion: ConversionModel,
    fee: FiatAmountModel
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.container
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.large),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Information(
                minimumSendAmount = minimumSendAmount,
                tokenUiConfig = tokenUiConfig,
                fundingAddress = fundingAddress,
                copyAddressClick = copyAddressClick,
                chainName = chainName
            )

            HorizontalDivider()

            VerticalSpacer { large }

            Address(
                fundingAddress = fundingAddress,
                copyAddressClick = copyAddressClick
            )

            VerticalSpacer { large }

            HorizontalDivider()

            VerticalSpacer { large }

            ConversionRate(conversion)

            VerticalSpacer { mediumIncreased }

            Fee(fee)
        }
    }
}

@Composable
private fun Fee(fee: FiatAmountModel) {
    Row(
        modifier = Modifier.fillMaxWidth()
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(R.string.fund_fee),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
        )

        NovaText(
            text = LocalFiatFormatter.current.formatFiatAmount(
                fiatAmount = fee,
                approx = true,
            ),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

@Composable
private fun ConversionRate(conversion: ConversionModel) {
    Row(
        modifier = Modifier.fillMaxWidth()
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(R.string.fund_rate),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
        )

        NovaText(
            text = LocalConversionFormatter.current.formatConversion(
                tokenAmountFrom = conversion.from,
                tokenAmountTo = conversion.to,
                precisionFrom = RoundPrecision.FIAT,
                precisionTo = RoundPrecision.HIGH,
                approx = true
            ).withCurrencyTickerStyle(PolkadotTheme.typography.body.large),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

@Composable
private fun Address(fundingAddress: String, copyAddressClick: (String) -> Unit) {
    QrCode(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.extraLargeIncreased),
        text = fundingAddress
    )

    VerticalSpacer { large }

    NovaText(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        text = fundingAddress,
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center
    )

    VerticalSpacer { large }

    PolkadotTextButton(
        text = stringResource(R.string.fund_copy_address),
        onClick = { copyAddressClick(fundingAddress) },
    )
}

@Composable
private fun Information(
    minimumSendAmount: TokenAmountModel,
    tokenUiConfig: KnownTokenUiConfig,
    fundingAddress: String,
    copyAddressClick: (String) -> Unit,
    chainName: String
) {
    SendingToken(minimumSendAmount, tokenUiConfig)

    VerticalSpacer { extraMedium }

    SendTo(fundingAddress, copyAddressClick)

    VerticalSpacer { extraMedium }

    Network(chainName)

    VerticalSpacer { large }
}

@Composable
private fun Network(chainName: String) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(R.string.fund_network),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
        )

        NovaText(
            text = chainName,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

@Composable
private fun SendTo(fundingAddress: String, copyAddressClick: (String) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(R.string.fund_to),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
        )

        NovaText(
            modifier = Modifier.width(124.dp),
            text = fundingAddress,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
            overflow = TextOverflow.MiddleEllipsis,
            maxLines = 1
        )

        HorizontalSpacer { small }

        IconButton(
            modifier = Modifier.size(32.dp),
            shape = PolkadotTheme.shapes.full,
            colors = IconButtonDefaults.iconButtonColors(containerColor = Color(0x0FFFFFFF)),
            onClick = { copyAddressClick(fundingAddress) }
        ) {
            NovaIcon(
                modifier = Modifier.size(16.dp),
                imageVector = NovaIcons.ContentCopy,
                tint = PolkadotTheme.colors.fg.primary
            )
        }
    }
}

@Composable
private fun SendingToken(
    minimumSendAmount: TokenAmountModel,
    tokenUiConfig: KnownTokenUiConfig
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically
    ) {
        val formattedAmount = LocalTokenAmountFormatter.current.formatTokenAmount(
            tokenAmount = minimumSendAmount,
            precision = RoundPrecision.DEFAULT
        )
        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(R.string.fund_minimal_amount, formattedAmount),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
        )

        TokenChip(tokenUiConfig)
    }
}

@Preview
@Composable
private fun FundingWidgetPreview() {
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

            FundingWidget(
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
                copyAddressClick = {},
                tokenUiConfig = KnownTokenUiConfig(
                    color = PolkadotTheme.colors.fg.warning,
                    icon = NovaIcons.Failure,
                    symbol = "DOT",
                    name = "Polkadot"
                )
            )
        }
    }
}
