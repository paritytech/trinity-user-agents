package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.loading.onLoading
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
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.ProductPermissionsViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.compose.components.ProductPermissionItem
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.models.ProductPermissionsUiModel
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.NativeMediaPermissionStatus
import io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.compose.components.NativeMediaPermissionItem
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.feature_products_impl.domain.productPermissions.AutomaticPreimagePermission
import uniffi.truapi.PermissionAuthorizationStatus
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ProductPermissionsScreen(viewModel: ProductPermissionsViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    ProductPermissionsScreenInternal(
        state = state,
        onBack = viewModel::onBack,
        onPermissionToggle = viewModel::onPermissionToggle,
        onMediaPermissionToggle = viewModel::onMediaPermissionToggle,
        onAutomaticUploadsChanged = viewModel::onAutomaticUploadsChanged,
    )
}

@Composable
private fun ProductPermissionsScreenInternal(
    state: LoadingState<ProductPermissionsUiModel>,
    onBack: () -> Unit,
    onPermissionToggle: (ProductPermissionStatus) -> Unit,
    onMediaPermissionToggle: (NativeMediaPermissionStatus) -> Unit,
    onAutomaticUploadsChanged: (AutomaticPreimagePermission, PermissionAuthorizationStatus) -> Unit,
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
                        item {
                            AutomaticUploadsItem(
                                permission = uiModel.automaticUploads,
                                busy = uiModel.automaticUploadsBusy,
                                onChanged = onAutomaticUploadsChanged,
                            )
                        }
                        items(uiModel.permissions) { permissionStatus ->
                            ProductPermissionItem(
                                permissionStatus = permissionStatus,
                                onToggle = { onPermissionToggle(permissionStatus) }
                            )
                        }
                        items(uiModel.mediaPermissions) { permission ->
                            NativeMediaPermissionItem(permission) { onMediaPermissionToggle(permission) }
                        }
                    }
                }
                .onLoading {
                    LoadingScreenState()
                }
        }
    }
}

@Composable
private fun AutomaticUploadsItem(
    permission: AutomaticPreimagePermission?,
    busy: Boolean,
    onChanged: (AutomaticPreimagePermission, PermissionAuthorizationStatus) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth().padding(PolkadotTheme.spacings.large)) {
        NovaText(
            text = stringResource(RCommon.string.product_permission_automatic_uploads),
            style = PolkadotTheme.typography.title.large,
            color = PolkadotTheme.colors.fg.primary,
        )
        VerticalSpacer { small }
        NovaText(
            text = if (permission == null) {
                stringResource(RCommon.string.product_permission_automatic_uploads_unavailable)
            } else {
                stringResource(
                    RCommon.string.product_permission_automatic_uploads_description,
                    permission.rootPublicKey.toHexString(withPrefix = true),
                    permission.genesisHash,
                )
            },
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.secondary,
        )
        if (permission != null) {
            val authorized = permission.status == PermissionAuthorizationStatus.AUTHORIZED
            VerticalSpacer { small }
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                enabled = !busy,
                text = stringResource(
                    if (authorized) RCommon.string.product_permission_automatic_uploads_revoke
                    else RCommon.string.product_permission_automatic_uploads_allow,
                ),
                style = PolkadotButtonStyle.secondary(),
                onClick = {
                    onChanged(
                        permission,
                        if (authorized) PermissionAuthorizationStatus.DENIED else PermissionAuthorizationStatus.AUTHORIZED,
                    )
                },
            )
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                enabled = !busy && permission.status != PermissionAuthorizationStatus.NOT_DETERMINED,
                text = stringResource(RCommon.string.product_permission_automatic_uploads_reset),
                style = PolkadotButtonStyle.ghost(),
                onClick = { onChanged(permission, PermissionAuthorizationStatus.NOT_DETERMINED) },
            )
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
                        ProductPermissionStatus(ProductPermission.DeviceCapability(DeviceCapabilityType.Camera), granted = true),
                        ProductPermissionStatus(ProductPermission.DeviceCapability(DeviceCapabilityType.Location), granted = true),
                        ProductPermissionStatus(ProductPermission.RemotePermission.NetworkAccess("api.example.com"), granted = false)
                    )
                )
            ),
            onBack = {},
            onPermissionToggle = { _ -> },
            onMediaPermissionToggle = { _ -> },
            onAutomaticUploadsChanged = { _, _ -> },
        )
    }
}
