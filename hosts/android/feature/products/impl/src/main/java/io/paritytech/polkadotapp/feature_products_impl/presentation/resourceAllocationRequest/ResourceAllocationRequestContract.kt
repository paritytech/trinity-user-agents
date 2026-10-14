package io.paritytech.polkadotapp.feature_products_impl.presentation.resourceAllocationRequest

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportDisplay
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow

interface ResourceAllocationRequestContract {
    val state: StateFlow<LoadingState<ResourceAllocationRequestUiState>>

    val stalenessReport: StalenessReportDisplay

    fun onApproveClicked()

    fun onRejectClicked()
}

data class ResourceAllocationRequestUiState(
    val productId: String,
    val resourceLabels: ImmutableList<Int>,
    val isAllocating: Boolean,
)
