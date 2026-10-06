package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.onError
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.loading.onLoading
import io.paritytech.polkadotapp.design.components.error.DefaultErrorState
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions.SavedProductPermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.ProductPermissionsViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.compose.components.ProductPermissionItem
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.models.ProductPermissionsUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ProductPermissionsScreen(viewModel: ProductPermissionsViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    ProductPermissionsScreenInternal(
        state = state,
        onBack = viewModel::onBack,
        onPermissionToggle = viewModel::onPermissionToggle
    )
}

@Composable
private fun ProductPermissionsScreenInternal(
    state: LoadingState<ProductPermissionsUiModel>,
    onBack: () -> Unit,
    onPermissionToggle: (SavedProductPermission) -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
        ) {
            state
                .onLoaded { uiModel ->
                    PolkadotTopBar(
                        title = stringResource(RCommon.string.product_permissions_toolbar_title, uiModel.productName),
                        navigationAction = rememberTopBarAction(action = onBack),
                        titleAlignment = TopBarTitleAlignment.Center,
                    )

                    VerticalSpacer { extraMedium }

                    LazyColumn(modifier = Modifier.fillMaxSize()) {
                        items(uiModel.permissions) { permissionStatus ->
                            ProductPermissionItem(
                                permissionStatus = permissionStatus,
                                onToggle = { onPermissionToggle(permissionStatus) }
                            )
                        }
                    }
                }
                .onLoading {
                    LoadingScreenState()
                }
                .onError {
                    PolkadotTopBar(
                        title = stringResource(RCommon.string.product_settings_permissions),
                        navigationAction = rememberTopBarAction(action = onBack),
                        titleAlignment = TopBarTitleAlignment.Center,
                    )
                    DefaultErrorState(stringResource(RCommon.string.generic_error_notification))
                }
        }
    }
}

@Preview
@Composable
private fun ProductPermissionsScreenPreview() {
    PolkadotTheme {
        ProductPermissionsScreenInternal(
            state = LoadingState.Loaded(
                ProductPermissionsUiModel(
                    productName = "Web3 Summit",
                    permissions = listOf(
                        SavedProductPermission.Legacy(ProductPermissionStatus(ProductPermission.DeviceCapability(DeviceCapabilityType.Camera), granted = true)),
                        SavedProductPermission.Legacy(ProductPermissionStatus(ProductPermission.DeviceCapability(DeviceCapabilityType.Location), granted = true)),
                        SavedProductPermission.Legacy(ProductPermissionStatus(ProductPermission.RemotePermission.NetworkAccess("api.example.com"), granted = false))
                    )
                )
            ),
            onBack = {},
            onPermissionToggle = { _ -> }
        )
    }
}
