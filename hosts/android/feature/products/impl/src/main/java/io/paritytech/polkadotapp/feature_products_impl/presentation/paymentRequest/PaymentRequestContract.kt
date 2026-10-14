package io.paritytech.polkadotapp.feature_products_impl.presentation.paymentRequest

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_products_impl.domain.paymentRequest.PaymentRequestStep
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import kotlinx.coroutines.flow.StateFlow

interface PaymentRequestContract {
    val state: StateFlow<LoadingState<PaymentRequestUiState>>

    fun onApproveClicked()

    fun onRejectClicked()
}

data class PaymentRequestUiState(
    val productId: String,
    val amount: TokenAmountModel,
    val step: PaymentRequestStep,
)
