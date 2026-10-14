package io.paritytech.polkadotapp.feature_products_impl.presentation.topUpRequest.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaBottomSheetSurface
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.feature_products_impl.presentation.topUpRequest.TopUpRequestUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.topUpRequest.TopUpRequestViewModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun TopUpRequestScreen(viewModel: TopUpRequestViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    state.onLoaded { data ->
        TopUpRequestScreenInternal(
            state = data,
            onDismiss = viewModel::onDismissClicked,
        )
    }
}

@Composable
private fun TopUpRequestScreenInternal(
    state: TopUpRequestUiState,
    onDismiss: () -> Unit,
) {
    NovaBottomSheetSurface {
        Column(
            modifier = Modifier.padding(
                top = PolkadotTheme.spacings.large,
                bottom = PolkadotTheme.spacings.mediumIncreased,
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.mediumIncreased,
            ),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            when (state) {
                is TopUpRequestUiState.Failure -> FailureContent(state)
                is TopUpRequestUiState.PartialPayment -> PartialPaymentContent(state)
            }

            VerticalSpacer { extraLarge }

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.common_close),
                style = PolkadotButtonStyle.secondary(),
                onClick = onDismiss,
            )
        }
    }
}

@Composable
private fun FailureContent(state: TopUpRequestUiState.Failure) {
    NovaText(
        text = stringResource(RCommon.string.product_top_up_failed_title, state.productId).withBold(state.productId),
        style = PolkadotTheme.typography.title.large,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center,
    )

    VerticalSpacer { mediumIncreased }

    NovaText(
        text = stringResource(RCommon.string.product_top_up_failed_message),
        style = PolkadotTheme.typography.body.medium,
        color = PolkadotTheme.colors.fg.error,
        textAlign = TextAlign.Center,
    )
}

@Composable
private fun PartialPaymentContent(state: TopUpRequestUiState.PartialPayment) {
    val formatter = LocalTokenAmountFormatter.current

    NovaText(
        text = stringResource(RCommon.string.product_top_up_accepted_title, state.productId).withBold(state.productId),
        style = PolkadotTheme.typography.title.large,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center,
    )

    VerticalSpacer { mediumIncreased }

    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        shape = PolkadotTheme.shapes.medium,
        color = PolkadotTheme.colors.bg.surface.nested,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            NovaText(
                text = formatter.formatTokenAmount(state.requestedAmount, RoundPrecision.DEFAULT, withSymbol = false),
                style = PolkadotTheme.typography.body.medium.copy(textDecoration = TextDecoration.LineThrough),
                color = PolkadotTheme.colors.fg.secondary,
                textAlign = TextAlign.Center,
            )
            NovaText(
                text = formatter.formatTokenAmount(state.creditedAmount, RoundPrecision.DEFAULT, withSymbol = false),
                style = PolkadotTheme.typography.headline.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center,
            )
            NovaText(
                text = formatter.formatToSymbol(state.creditedAmount).withCurrencyTickerStyle(PolkadotTheme.typography.title.large),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.secondary,
                textAlign = TextAlign.Center,
            )
        }
    }

    VerticalSpacer { mediumIncreased }

    NovaText(
        text = stringResource(RCommon.string.product_top_up_amount_mismatch_warning),
        style = PolkadotTheme.typography.body.medium,
        color = PolkadotTheme.colors.fg.warning,
        textAlign = TextAlign.Center,
    )
}

@Preview
@Composable
private fun TopUpRequestFailurePreview() {
    PolkadotTheme {
        TopUpRequestScreenInternal(
            state = TopUpRequestUiState.Failure(productId = "alice.dot"),
            onDismiss = {},
        )
    }
}

@Preview
@Composable
private fun TopUpRequestPartialPaymentPreview() {
    CompositionLocalProvider(
        LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
        LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
    ) {
        PolkadotTheme {
            TopUpRequestScreenInternal(
                state = TopUpRequestUiState.PartialPayment(
                    productId = "alice.dot",
                    requestedAmount = TokenAmountModel.mock,
                    creditedAmount = TokenAmountModel.mock,
                ),
                onDismiss = {},
            )
        }
    }
}
