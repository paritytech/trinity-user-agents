package io.paritytech.polkadotapp.feature_products_impl.presentation.paymentRequest.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
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
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_coinage_api.presentation.privacy.ConfirmGainingPrivacySpendContent
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestStep
import io.paritytech.polkadotapp.feature_products_impl.presentation.paymentRequest.PaymentRequestContract
import io.paritytech.polkadotapp.feature_products_impl.presentation.paymentRequest.PaymentRequestUiState
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun PaymentRequestScreen(contract: PaymentRequestContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    state.onLoaded { data ->
        PaymentRequestScreenInternal(
            state = data,
            onApprove = contract::onApproveClicked,
            onReject = contract::onRejectClicked,
        )
    }
}

@Composable
private fun PaymentRequestScreenInternal(
    state: PaymentRequestUiState,
    onApprove: () -> Unit,
    onReject: () -> Unit,
) {
    NovaBottomSheetSurface {
        when (state.step) {
            PaymentRequestStep.Confirm -> PaymentRequestConfirmContent(state, onApprove, onReject)

            PaymentRequestStep.PrivacyWarning -> ConfirmGainingPrivacySpendContent(
                totalTransfer = state.amount,
                onSendAnyway = onApprove,
                onCancel = onReject,
            )
        }
    }
}

@Composable
private fun PaymentRequestConfirmContent(
    state: PaymentRequestUiState,
    onApprove: () -> Unit,
    onReject: () -> Unit,
) {
    Column(
        modifier = Modifier.padding(
            top = PolkadotTheme.spacings.large,
            bottom = PolkadotTheme.spacings.mediumIncreased,
            start = PolkadotTheme.spacings.mediumIncreased,
            end = PolkadotTheme.spacings.mediumIncreased,
        ),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        val formatter = LocalTokenAmountFormatter.current
        NovaText(
            text = stringResource(
                RCommon.string.product_payment_request_title,
                state.productId,
                formatter.formatTokenAmount(state.amount, precision = RoundPrecision.DEFAULT),
            ).withCurrencyTickerStyle(PolkadotTheme.typography.title.large),
            style = PolkadotTheme.typography.title.large,
            color = PolkadotTheme.colors.fg.primary,
        )

        VerticalSpacer { extraLarge }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.product_payment_request_approve),
            style = PolkadotButtonStyle.secondary(),
            onClick = onApprove,
        )

        VerticalSpacer { small }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.product_payment_request_reject),
            style = PolkadotButtonStyle.ghost(),
            onClick = onReject,
        )
    }
}

@Preview
@Composable
private fun PaymentRequestScreenPreview() {
    CompositionLocalProvider(
        LocalTokenAmountFormatter provides TokenAmountFormatter.mocked,
        LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
    ) {
        PolkadotTheme {
            PaymentRequestScreenInternal(
                state = PaymentRequestUiState(
                    productId = "alice.dot",
                    amount = TokenAmountModel.mock,
                    step = PaymentRequestStep.Confirm,
                ),
                onApprove = {},
                onReject = {},
            )
        }
    }
}
