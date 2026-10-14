package io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Bluetooth
import io.paritytech.polkadotapp.design.components.icon.vectors.CallFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.CloudOn
import io.paritytech.polkadotapp.design.components.icon.vectors.ContentCopy
import io.paritytech.polkadotapp.design.components.icon.vectors.Dollar
import io.paritytech.polkadotapp.design.components.icon.vectors.Fingerprint
import io.paritytech.polkadotapp.design.components.icon.vectors.Location
import io.paritytech.polkadotapp.design.components.icon.vectors.MicOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.NFC
import io.paritytech.polkadotapp.design.components.icon.vectors.NotificationsBellOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.PeopleOutline
import io.paritytech.polkadotapp.design.components.icon.vectors.Photocam
import io.paritytech.polkadotapp.design.components.icon.vectors.Send
import io.paritytech.polkadotapp.design.components.icon.vectors.Share
import io.paritytech.polkadotapp.design.components.icon.vectors.WiFi
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.common.R as RCommon

internal val ProductPermission.icon: ImageVector
    get() = when (this) {
        is ProductPermission.DeviceCapability -> capability.icon
        is ProductPermission.AccountAccess -> NovaIcons.PeopleOutline
        is ProductPermission.BalanceAccess -> NovaIcons.Dollar
        is ProductPermission.UserIdentityAccess -> NovaIcons.PeopleOutline
        is ProductPermission.RemotePermission.NetworkAccess -> NovaIcons.WiFi
        ProductPermission.RemotePermission.WebRtcAccess -> NovaIcons.CallFilled
        ProductPermission.RemotePermission.ChainSubmitAccess -> NovaIcons.Send
        ProductPermission.RemotePermission.StatementSubmitAccess -> NovaIcons.CloudOn
        ProductPermission.RemotePermission.PreimageSubmitAccess -> NovaIcons.CloudOn
    }

private val DeviceCapabilityType.icon: ImageVector
    get() = when (this) {
        DeviceCapabilityType.Camera -> NovaIcons.Photocam
        DeviceCapabilityType.Microphone -> NovaIcons.MicOutlined
        DeviceCapabilityType.Bluetooth -> NovaIcons.Bluetooth
        DeviceCapabilityType.NFC -> NovaIcons.NFC
        DeviceCapabilityType.Location -> NovaIcons.Location
        DeviceCapabilityType.Notifications -> NovaIcons.NotificationsBellOutlined
        DeviceCapabilityType.Clipboard -> NovaIcons.ContentCopy
        DeviceCapabilityType.Biometrics -> NovaIcons.Fingerprint
        DeviceCapabilityType.OpenUrl -> NovaIcons.Share
    }

@Composable
internal fun ProductPermission.title(productId: String): String {
    return when (this) {
        is ProductPermission.AccountAccess -> stringResource(RCommon.string.product_permission_account_title, productId, targetProductId)
        is ProductPermission.BalanceAccess -> stringResource(RCommon.string.product_permission_balance_title, productId)
        is ProductPermission.UserIdentityAccess -> stringResource(RCommon.string.product_permission_user_identity_title, productId)
        is ProductPermission.DeviceCapability -> capability.title(productId)
        is ProductPermission.RemotePermission.NetworkAccess -> stringResource(RCommon.string.product_permission_network_title, productId, domain)
        ProductPermission.RemotePermission.WebRtcAccess -> stringResource(RCommon.string.product_permission_webrtc_title, productId)
        ProductPermission.RemotePermission.ChainSubmitAccess -> stringResource(RCommon.string.product_permission_chain_submit_title, productId)
        ProductPermission.RemotePermission.StatementSubmitAccess -> stringResource(RCommon.string.product_permission_statement_submit_title, productId)
        ProductPermission.RemotePermission.PreimageSubmitAccess -> stringResource(RCommon.string.product_permission_preimage_submit_title, productId)
    }
}

@Composable
private fun DeviceCapabilityType.title(productId: String): String {
    val resId = when (this) {
        DeviceCapabilityType.Camera -> RCommon.string.product_permission_device_camera_title
        DeviceCapabilityType.Bluetooth -> RCommon.string.product_permission_device_bluetooth_title
        DeviceCapabilityType.Microphone -> RCommon.string.product_permission_device_microphone_title
        DeviceCapabilityType.Notifications -> RCommon.string.product_permission_device_notifications_title
        DeviceCapabilityType.NFC -> RCommon.string.product_permission_device_nfc_title
        DeviceCapabilityType.Location -> RCommon.string.product_permission_device_location_title
        DeviceCapabilityType.Clipboard -> RCommon.string.product_permission_device_clipboard_title
        DeviceCapabilityType.Biometrics -> RCommon.string.product_permission_device_biometrics_title
        DeviceCapabilityType.OpenUrl -> RCommon.string.product_permission_device_open_url_title
    }
    return stringResource(resId, productId)
}

@Composable
internal fun ProductPermission.subtitle(): String {
    val manageLater = stringResource(RCommon.string.product_permission_subtitle)
    return when (this) {
        is ProductPermission.AccountAccess -> stringResource(RCommon.string.product_permission_account_subtitle, manageLater)
        is ProductPermission.BalanceAccess -> stringResource(RCommon.string.product_permission_balance_subtitle, manageLater)
        is ProductPermission.UserIdentityAccess -> stringResource(RCommon.string.product_permission_user_identity_subtitle, manageLater)
        is ProductPermission.DeviceCapability -> capability.subtitle(manageLater)
        is ProductPermission.RemotePermission.NetworkAccess -> stringResource(RCommon.string.product_permission_network_subtitle, manageLater)
        ProductPermission.RemotePermission.WebRtcAccess -> stringResource(RCommon.string.product_permission_webrtc_subtitle, manageLater)
        ProductPermission.RemotePermission.ChainSubmitAccess -> stringResource(RCommon.string.product_permission_chain_submit_subtitle, manageLater)
        ProductPermission.RemotePermission.StatementSubmitAccess -> stringResource(RCommon.string.product_permission_statement_submit_subtitle, manageLater)
        ProductPermission.RemotePermission.PreimageSubmitAccess -> stringResource(RCommon.string.product_permission_preimage_submit_subtitle, manageLater)
    }
}

@Composable
private fun DeviceCapabilityType.subtitle(manageLater: String): String {
    val resId = when (this) {
        DeviceCapabilityType.Camera -> RCommon.string.product_permission_device_camera_subtitle
        DeviceCapabilityType.Bluetooth -> RCommon.string.product_permission_device_bluetooth_subtitle
        DeviceCapabilityType.Microphone -> RCommon.string.product_permission_device_microphone_subtitle
        DeviceCapabilityType.Notifications -> RCommon.string.product_permission_device_notifications_subtitle
        DeviceCapabilityType.NFC -> RCommon.string.product_permission_device_nfc_subtitle
        DeviceCapabilityType.Location -> RCommon.string.product_permission_device_location_subtitle
        DeviceCapabilityType.Clipboard -> RCommon.string.product_permission_device_clipboard_subtitle
        DeviceCapabilityType.Biometrics -> RCommon.string.product_permission_device_biometrics_subtitle
        DeviceCapabilityType.OpenUrl -> RCommon.string.product_permission_device_open_url_subtitle
    }
    return stringResource(resId, manageLater)
}

@Composable
internal fun ProductPermission.RemotePermission.shortLabel(): String {
    return when (this) {
        is ProductPermission.RemotePermission.NetworkAccess -> stringResource(RCommon.string.product_permission_network_label, domain)
        ProductPermission.RemotePermission.WebRtcAccess -> stringResource(RCommon.string.product_permission_webrtc_label)
        ProductPermission.RemotePermission.ChainSubmitAccess -> stringResource(RCommon.string.product_permission_chain_submit_label)
        ProductPermission.RemotePermission.StatementSubmitAccess -> stringResource(RCommon.string.product_permission_statement_submit_label)
        ProductPermission.RemotePermission.PreimageSubmitAccess -> stringResource(RCommon.string.product_permission_preimage_submit_label)
    }
}
