package io.paritytech.polkadotapp.feature_products_impl.presentation.paymentRequest

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.paymentRequest.compose.PaymentRequestScreen
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import javax.inject.Inject

@AndroidEntryPoint
class PaymentRequestBottomSheet : BaseComposeBottomSheet<PaymentRequestViewModel>() {
    @Inject
    lateinit var tokenAmountFormatter: TokenAmountFormatter

    override val viewModel: PaymentRequestViewModel by viewModels()

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    @Composable
    override fun Screen() = CompositionLocalProvider(
        LocalTokenAmountFormatter provides tokenAmountFormatter
    ) {
        PaymentRequestScreen(viewModel)
    }
}
