package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions

import androidx.lifecycle.SavedStateHandle
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.ProductSettingsPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions.ProductPermissionsInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions.AutomaticPreimagePermission
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.catch
import uniffi.truapi.PermissionAuthorizationStatus
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.models.ProductPermissionsUiModel
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.NativeMediaPermissionStatus
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
    private val automaticUploadsBusy = MutableStateFlow(false)

    val state: StateFlow<LoadingState<ProductPermissionsUiModel>> = combine(
        productFlow,
        interactor.observePermissions(productId),
        interactor.observeMediaPermissions(productId),
        interactor.observeAutomaticUploads(productId).catch { emit(null) },
        automaticUploadsBusy,
    ) { product, permissions, mediaPermissions, automaticUploads, busy ->
        ProductPermissionsUiModel(
            productName = product?.name.orEmpty(),
            permissions = permissions,
            mediaPermissions = mediaPermissions,
            automaticUploads = automaticUploads,
            automaticUploadsBusy = busy,
        )
    }
        .withLoading()
        .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    fun onBack() {
        router.back()
    }

    fun onPermissionToggle(permissionStatus: ProductPermissionStatus) = launchUnit {
        runCatching { interactor.togglePermission(productId, permissionStatus) }
            .onFailure {
                if (it is CancellationException) throw it
                showPresentationError(UnexpectedPresentationError(it))
            }
    }

    fun onMediaPermissionToggle(permissionStatus: NativeMediaPermissionStatus) = launchUnit {
        runCatching { interactor.toggleMediaPermission(productId, permissionStatus) }
            .onFailure {
                if (it is CancellationException) throw it
                showPresentationError(UnexpectedPresentationError(it))
            }
    }

    fun onAutomaticUploadsChanged(
        rendered: AutomaticPreimagePermission,
        status: PermissionAuthorizationStatus,
    ) = launchUnit {
        if (automaticUploadsBusy.value) return@launchUnit
        automaticUploadsBusy.value = true
        try {
            interactor.setAutomaticUploads(productId, rendered, status)
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            showError(error)
        } finally {
            automaticUploadsBusy.value = false
        }
    }
}
