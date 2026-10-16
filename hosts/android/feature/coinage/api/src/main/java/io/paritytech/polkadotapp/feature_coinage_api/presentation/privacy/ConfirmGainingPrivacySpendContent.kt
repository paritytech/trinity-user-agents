package io.paritytech.polkadotapp.feature_coinage_api.presentation.privacy

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.formatFiat
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import java.math.BigDecimal
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ConfirmGainingPrivacySpendContent(
    modifier: Modifier = Modifier,
    totalTransfer: TokenAmountModel,
    onSendAnyway: () -> Unit,
    onCancel: () -> Unit,
) {
    val totalAmountText = LocalTokenAmountFormatter.current.formatFiat(totalTransfer)

    Column(
        modifier = modifier
            .fillMaxWidth()
            .padding(all = PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        VerticalSpacer { small }

        NovaIcon(
            modifier = Modifier.size(WARNING_ICON_SIZE),
            imageVector = NovaIcons.Info,
            tint = PolkadotTheme.colors.fg.warning
        )

        VerticalSpacer { mediumIncreased }

        NovaText(
            text = stringResource(RCommon.string.send_gaining_privacy_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )

        VerticalSpacer { small }

        NovaText(
            text = stringResource(RCommon.string.send_gaining_privacy_full_amount),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.secondary,
            textAlign = TextAlign.Center,
        )

        VerticalSpacer { extraLarge }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.send_gaining_privacy_send_anyway_button, totalAmountText)
                .withCurrencyTickerStyle(PolkadotTheme.typography.title.large),
            style = PolkadotButtonStyle.tertiary(),
            onClick = onSendAnyway,
        )

        VerticalSpacer { small }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.common_cancel),
            style = PolkadotButtonStyle.ghost(),
            onClick = onCancel,
        )
    }
}

private val WARNING_ICON_SIZE = 50.dp

@Preview
@Composable
private fun ConfirmGainingPrivacySpendContentPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
            LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
        ) {
            NovaBottomSheetSurface {
                ConfirmGainingPrivacySpendContent(
                    totalTransfer = object : TokenAmountModel {
                        override val amount: BigDecimal = 100.toBigDecimal()
                        override val appearance: TokenSymbolAppearance = TokenSymbolAppearance.DigitalDollar
                    },
                    onSendAnyway = {},
                    onCancel = {},
                )
            }
        }
    }
}
