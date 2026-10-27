package io.paritytech.polkadotapp.feature_products_impl.presentation.topUpRequest

import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import uniffi.truapi.ProductId

sealed interface TopUpRequestUiState {
    val productId: ProductId

    /** Nothing was claimed, and nothing more will be tried. */
    data class Failure(override val productId: ProductId) : TopUpRequestUiState

    /** The claim succeeded but [creditedAmount] funds were accepted instead of [requestedAmount]. */
    data class PartialPayment(
        override val productId: ProductId,
        val requestedAmount: TokenAmountModel,
        val creditedAmount: TokenAmountModel,
    ) : TopUpRequestUiState
}
