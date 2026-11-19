package io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan

import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.truapiProductScan.compose.TrUAPIProductScanScreen

/**
 * A full-height sheet rather than a screen: opening a screen would close any sheet under it,
 * including the product sheet whose product asked to scan.
 */
@AndroidEntryPoint
class TrUAPIProductScanBottomSheet : BaseComposeBottomSheet<TrUAPIProductScanViewModel>() {
    override val viewModel: TrUAPIProductScanViewModel by viewModels()

    @Composable
    override fun Screen() = TrUAPIProductScanScreen(viewModel)

    override fun onStart() {
        super.onStart()
        bottomSheetBehavior?.isDraggable = false
        val content = view ?: return
        content.layoutParams.height = MATCH_PARENT
        (content.parent as? View)?.layoutParams?.height = MATCH_PARENT
    }
}
