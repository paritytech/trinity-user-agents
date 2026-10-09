package io.paritytech.polkadotapp.feature_products_impl.domain.permissions

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.AllowanceAccountSelector
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.normalizeProductId
import uniffi.truapi.DerivationIndex
import uniffi.truapi.HostDevicePermissionRequest
import uniffi.truapi.PermissionAuthorizationRequest
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest
import okhttp3.HttpUrl.Companion.toHttpUrl
import java.util.Locale

internal fun ProductPermission.canonicalRequest(): PermissionAuthorizationRequest? = when (this) {
    ProductPermission.BalanceAccess -> null
    ProductPermission.UserIdentityAccess -> PermissionAuthorizationRequest.IdentityDisclosure
    ProductPermission.ChatAuthority -> PermissionAuthorizationRequest.ChatAuthority
    ProductPermission.ProfileDisclosure -> PermissionAuthorizationRequest.ProfileDisclosure
    is ProductPermission.StatementStoreAllowance -> PermissionAuthorizationRequest.StatementStoreAllowance(
        when (val selector = derivationIndex) {
            null -> null
            is AllowanceAccountSelector.Index -> DerivationIndex.Index(selector.value)
            is AllowanceAccountSelector.Raw -> DerivationIndex.Raw(selector.value.value)
        }
    )
    is ProductPermission.AccountAccess -> PermissionAuthorizationRequest.AccountAccess(bareProductLabel(targetProductId))
    is ProductPermission.DeviceCapability -> PermissionAuthorizationRequest.Device(capability.native())
    is ProductPermission.RemotePermission -> PermissionAuthorizationRequest.Remote(RemotePermissionRequest(when (this) {
        is ProductPermission.RemotePermission.NetworkAccess -> RemotePermission.Remote(listOf(canonicalDomain(domain)))
        is ProductPermission.RemotePermission.NetworkAccessSet -> RemotePermission.Remote(domains.map(::canonicalDomain).distinct().sorted())
        ProductPermission.RemotePermission.WebRtcAccess -> RemotePermission.WebRtc
        ProductPermission.RemotePermission.ChainSubmitAccess -> RemotePermission.ChainSubmit
        ProductPermission.RemotePermission.StatementSubmitAccess -> RemotePermission.StatementSubmit
        ProductPermission.RemotePermission.PreimageSubmitAccess -> RemotePermission.PreimageSubmit
        is ProductPermission.RemotePermission.JamPeersAccess -> RemotePermission.JamPeers(genesis.fromHex().also {
            require(it.size == 32) { "JAM genesis must contain 32 bytes" }
        })
    }))
}

/** Account-scoped Media and upload permissions have their own settings surfaces. */
internal fun PermissionAuthorizationRequest.hasDedicatedSettings(): Boolean = when (this) {
    is PermissionAuthorizationRequest.Calling, is PermissionAuthorizationRequest.AutomaticPreimageSubmit -> true
    is PermissionAuthorizationRequest.Remote -> v1.permission == RemotePermission.Calling
    else -> false
}

internal fun PermissionAuthorizationRequest.legacyPermission(): ProductPermission = when (this) {
    is PermissionAuthorizationRequest.Calling, is PermissionAuthorizationRequest.AutomaticPreimageSubmit ->
        error("Account-scoped permission requires dedicated settings")
    is PermissionAuthorizationRequest.Device -> ProductPermission.DeviceCapability(DeviceCapabilityType.entries.single { it.native() == v1 })
    is PermissionAuthorizationRequest.AccountAccess -> ProductPermission.AccountAccess(targetProductId)
    PermissionAuthorizationRequest.IdentityDisclosure -> ProductPermission.UserIdentityAccess
    PermissionAuthorizationRequest.ChatAuthority -> ProductPermission.ChatAuthority
    PermissionAuthorizationRequest.ProfileDisclosure -> ProductPermission.ProfileDisclosure
    is PermissionAuthorizationRequest.StatementStoreAllowance -> ProductPermission.StatementStoreAllowance(
        when (val selector = derivationIndex) {
            null -> null
            is DerivationIndex.Index -> AllowanceAccountSelector.Index(selector.v1)
            is DerivationIndex.Raw -> AllowanceAccountSelector.Raw(selector.v1.toDataByteArray())
        }
    )
    is PermissionAuthorizationRequest.Remote -> when (val remote = v1.permission) {
        is RemotePermission.Remote -> {
            val domains = remote.domains.map(::canonicalDomain).distinct().sorted()
            require(domains.isNotEmpty()) { "Cannot display an empty canonical network permission" }
            if (domains.size == 1) ProductPermission.RemotePermission.NetworkAccess(domains.single())
                else ProductPermission.RemotePermission.NetworkAccessSet(domains)
        }
        RemotePermission.WebRtc -> ProductPermission.RemotePermission.WebRtcAccess
        RemotePermission.Calling -> error("Calling requires scoped Media settings")
        RemotePermission.ChainSubmit -> ProductPermission.RemotePermission.ChainSubmitAccess
        RemotePermission.StatementSubmit -> ProductPermission.RemotePermission.StatementSubmitAccess
        RemotePermission.PreimageSubmit -> ProductPermission.RemotePermission.PreimageSubmitAccess
        is RemotePermission.JamPeers -> ProductPermission.RemotePermission.JamPeersAccess(remote.genesis.toHexString(withPrefix = true))
    }
}

private fun DeviceCapabilityType.native(): HostDevicePermissionRequest = when (this) {
    DeviceCapabilityType.Notifications -> HostDevicePermissionRequest.NOTIFICATIONS
    DeviceCapabilityType.Camera -> HostDevicePermissionRequest.CAMERA
    DeviceCapabilityType.Microphone -> HostDevicePermissionRequest.MICROPHONE
    DeviceCapabilityType.Bluetooth -> HostDevicePermissionRequest.BLUETOOTH
    DeviceCapabilityType.NFC -> HostDevicePermissionRequest.NFC
    DeviceCapabilityType.Location -> HostDevicePermissionRequest.LOCATION
    DeviceCapabilityType.Clipboard -> HostDevicePermissionRequest.CLIPBOARD
    DeviceCapabilityType.Biometrics -> HostDevicePermissionRequest.BIOMETRICS
    DeviceCapabilityType.OpenUrl -> HostDevicePermissionRequest.OPEN_URL
}

internal fun canonicalDomain(domain: String): String {
    val value = domain.trim().removeSuffix(".").lowercase(Locale.ROOT)
    if (value == "*") return value
    val wildcard = value.startsWith("*.")
    val host = if (wildcard) value.removePrefix("*.") else value
    require(host.isNotEmpty() && host.none { it in "/?#@*" }) { "Unsupported permission domain: $domain" }
    val url = "http://$host/".toHttpUrl()
    require(url.port == 80 && (':' !in host || host.startsWith('['))) { "Permission patterns cannot contain ports: $domain" }
    val canonical = url.host.let { if (':' in it) "[$it]" else it }
    require(!wildcard || ('.' in canonical && canonical.any { it.isLetter() } && ':' !in canonical)) {
        "Unsupported wildcard permission domain: $domain"
    }
    return (if (wildcard) "*." else "") + canonical
}

/** Mirrors the core's account-context key, shared by executable subnames. */
internal fun bareProductLabel(productId: String): String {
    val normalized = normalizeProductId(productId)
    return normalized.substringBeforeLast('.', normalized).substringAfterLast('.')
}
