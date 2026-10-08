package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction

import android.os.Bundle
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.compose.TransactionSignScreen

@AndroidEntryPoint
class TransactionSignBottomSheet : BaseComposeBottomSheet<TransactionSignViewModel>() {
    override val viewModel: TransactionSignViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    override fun onResume() {
        super.onResume()
        viewModel.onResume()
    }

    @Composable
    override fun Screen() = TransactionSignScreen(viewModel)

    companion object {
        const val REQUEST_ID = "signingRequestId"
    }
}
