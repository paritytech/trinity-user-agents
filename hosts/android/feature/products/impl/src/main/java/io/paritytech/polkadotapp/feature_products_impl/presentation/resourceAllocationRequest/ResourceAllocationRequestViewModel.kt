package io.paritytech.polkadotapp.feature_products_impl.presentation.resourceAllocationRequest

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReport
import io.paritytech.polkadotapp.common.utils.progressStallReport.launchWithDiagnostics
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContext
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.collections.immutable.toImmutableList
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

@HiltViewModel
class ResourceAllocationRequestViewModel @Inject constructor(
    private val router: ProductsRouter,
    private val context: ResourceAllocationRequestContext,
    private val holder: ResourceAllocationRequestContextHolder,
    private val interactor: ResourceAllocationRequestInteractor,
) : BaseViewModel(), ResourceAllocationRequestContract {
    override val stalenessReport = StalenessReport(this)

    private val resourceLabels = context.resources.map { it.labelRes() }.toImmutableList()

    private val allocating = MutableStateFlow(false)

    override val state: StateFlow<LoadingState<ResourceAllocationRequestUiState>> = allocating
        .map { isAllocating ->
            ResourceAllocationRequestUiState(
                productId = context.productId.value,
                resourceLabels = resourceLabels,
                isAllocating = isAllocating,
            )
        }
        .withLoading("ResourceAllocationRequest")
        .inBackground()
        .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    override fun onApproveClicked() {
        if (allocating.value) return
        allocating.value = true

        launchWithDiagnostics(stalenessReport) {
            interactor.allocateAll(context.productId, context.resources, context.onExisting)
                .onSuccess(context::deliver)
                // The product is waiting on this prompt alone, so an unexpected failure still has to answer it
                .onFailure { context.deliverAll(ApAllocationOutcome.NotAvailable) }

            router.back()
        }
    }

    override fun onRejectClicked() = launchUnit {
        context.deliverAll(ApAllocationOutcome.Rejected)
        router.back()
    }

    override fun onCleared() {
        super.onCleared()
        // This prompt owns the allocation, so once it is gone nobody else is left to answer the product
        context.deliverAll(ApAllocationOutcome.NotAvailable)
        holder.clear()
    }
}

private fun ApAllocatableResource.labelRes(): Int = when (this) {
    ApAllocatableResource.BulletInAllowance -> RCommon.string.product_resource_allocation_bulletin
    ApAllocatableResource.StatementStoreAllowance -> RCommon.string.product_resource_allocation_statement_store
    is ApAllocatableResource.SmartContractAllowance -> RCommon.string.product_resource_allocation_smart_contract
    ApAllocatableResource.AutoSigning -> RCommon.string.product_resource_allocation_auto_signing
}
