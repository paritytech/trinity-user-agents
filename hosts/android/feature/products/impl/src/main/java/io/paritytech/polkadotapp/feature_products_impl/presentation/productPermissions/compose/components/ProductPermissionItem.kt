package io.paritytech.polkadotapp.feature_products_impl.presentation.productPermissions.compose.components

import androidx.annotation.StringRes
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.compound.NovaSwitch
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermissionStatus
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun ProductPermissionItem(
    permissionStatus: ProductPermissionStatus,
    onToggle: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onToggle)
            .padding(
                horizontal = PolkadotTheme.spacings.large,
                vertical = PolkadotTheme.spacings.extraMedium
            ),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = permissionStatus.permission.displayName(),
                style = PolkadotTheme.typography.title.large,
                color = PolkadotTheme.colors.fg.primary
            )

            NovaText(
                text = permissionStatus.permission.displayDescription(),
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.tertiary
            )
        }

        NovaSwitch(
            checked = permissionStatus.granted,
            onCheckedChange = { onToggle() }
        )
    }
}

@Composable
private fun ProductPermission.displayName(): String {
    return when (this) {
        is ProductPermission.DeviceCapability -> stringResource(capability.nameRes())
        is ProductPermission.AccountAccess -> targetProductId
        is ProductPermission.BalanceAccess -> stringResource(RCommon.string.product_permission_type_balance_access)
        is ProductPermission.UserIdentityAccess -> stringResource(RCommon.string.product_permission_type_user_identity_access)
        is ProductPermission.RemotePermission.NetworkAccess -> domain
        ProductPermission.RemotePermission.WebRtcAccess -> stringResource(RCommon.string.product_permission_type_webrtc)
        ProductPermission.RemotePermission.ChainSubmitAccess -> stringResource(RCommon.string.product_permission_type_chain_submit)
        ProductPermission.RemotePermission.StatementSubmitAccess -> stringResource(RCommon.string.product_permission_type_statement_submit)
        ProductPermission.RemotePermission.PreimageSubmitAccess -> stringResource(RCommon.string.product_permission_type_preimage_submit)
    }
}

@Composable
private fun ProductPermission.displayDescription(): String {
    return stringResource(descriptionRes())
}

@StringRes
private fun DeviceCapabilityType.nameRes(): Int = when (this) {
    DeviceCapabilityType.Notifications -> RCommon.string.product_permission_capability_notifications
    DeviceCapabilityType.Camera -> RCommon.string.product_permission_capability_camera
    DeviceCapabilityType.Microphone -> RCommon.string.product_permission_capability_microphone
    DeviceCapabilityType.Bluetooth -> RCommon.string.product_permission_capability_bluetooth
    DeviceCapabilityType.NFC -> RCommon.string.product_permission_capability_nfc
    DeviceCapabilityType.Location -> RCommon.string.product_permission_capability_location
    DeviceCapabilityType.Clipboard -> RCommon.string.product_permission_capability_clipboard
    DeviceCapabilityType.Biometrics -> RCommon.string.product_permission_capability_biometrics
    DeviceCapabilityType.OpenUrl -> RCommon.string.product_permission_capability_open_url
}

@StringRes
private fun ProductPermission.descriptionRes(): Int = when (this) {
    is ProductPermission.DeviceCapability -> RCommon.string.product_permission_type_device_capability
    is ProductPermission.AccountAccess -> RCommon.string.product_permission_type_account_access
    is ProductPermission.BalanceAccess -> RCommon.string.product_permission_type_balance_access_description
    is ProductPermission.UserIdentityAccess -> RCommon.string.product_permission_type_user_identity_access_description
    is ProductPermission.RemotePermission.NetworkAccess -> RCommon.string.product_permission_type_network_access
    ProductPermission.RemotePermission.WebRtcAccess,
    ProductPermission.RemotePermission.ChainSubmitAccess,
    ProductPermission.RemotePermission.StatementSubmitAccess,
    ProductPermission.RemotePermission.PreimageSubmitAccess -> RCommon.string.product_permission_type_remote
}
