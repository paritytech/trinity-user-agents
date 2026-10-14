package io.paritytech.polkadotapp.feature_products_impl.presentation.resourceAllocationRequest

import android.os.Bundle
import android.view.View
import androidx.compose.runtime.Composable
import androidx.fragment.app.viewModels
import dagger.hilt.android.AndroidEntryPoint
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeBottomSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.resourceAllocationRequest.compose.ResourceAllocationRequestScreen

@AndroidEntryPoint
class ResourceAllocationRequestBottomSheet : BaseComposeBottomSheet<ResourceAllocationRequestViewModel>() {
    override val viewModel: ResourceAllocationRequestViewModel by viewModels()

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        isCancelable = false
        bottomSheetBehavior?.isDraggable = false
    }

    @Composable
    override fun Screen() {
        ResourceAllocationRequestScreen(viewModel)
    }
}
