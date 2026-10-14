package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.ProductSettingsPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions.ProductPermissionsInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.models.ProductPermissionsUiModel
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import javax.inject.Inject

@HiltViewModel
class ProductPermissionsViewModel @Inject constructor(
    savedStateHandle: SavedStateHandle,
    private val interactor: ProductPermissionsInteractor,
    private val router: ProductsRouter
) : BaseViewModel() {
    private val payload: ProductSettingsPayload = savedStateHandle.getPayload()
    private val productId = ProductId.fromStoredValue(payload.productId)

    private val productFlow = flowOf { interactor.getProduct(productId) }

    val state: StateFlow<LoadingState<ProductPermissionsUiModel>> = combine(
        productFlow,
        interactor.observePermissions(productId)
    ) { product, permissions ->
        ProductPermissionsUiModel(
            productName = product?.name.orEmpty(),
            permissions = permissions
        )
    }
        .withLoading()
        .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    fun onBack() {
        router.back()
    }

    fun onPermissionToggle(permissionStatus: ProductPermissionStatus) = launchUnit {
        interactor.togglePermission(productId, permissionStatus)
    }
}
