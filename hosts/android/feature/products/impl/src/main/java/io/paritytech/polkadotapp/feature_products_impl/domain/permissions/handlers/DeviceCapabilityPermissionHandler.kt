package io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.permissions.ForegroundPrompt
import io.paritytech.polkadotapp.common.utils.permissions.PermissionAsker
import io.paritytech.polkadotapp.common.utils.permissions.PermissionResult
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRequester
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.PermissionDecision
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import javax.inject.Inject

class DeviceCapabilityPermissionHandler @Inject constructor(
    private val repository: ProductPermissionRepository,
    private val requester: ProductPermissionRequester,
    private val permissionAsker: PermissionAsker,
    @param:ApplicationContext private val context: Context,
    private val foregroundPrompt: ForegroundPrompt,
) : ProductPermissionHandler<ProductPermission.DeviceCapability> {
    override suspend fun isGranted(productId: ProductId, permission: ProductPermission.DeviceCapability): Boolean {
        return repository.isGranted(productId, permission)
    }

    override suspend fun request(productId: ProductId, permission: ProductPermission.DeviceCapability): Boolean {
        if (isGranted(productId, permission)) {
            return requestOsPermissionIfNeeded(permission.capability)
        }

        val decision = requester.prompt(productId, permission)
        if (decision == PermissionDecision.Deny) return false

        val osGranted = requestOsPermissionIfNeeded(permission.capability)
        if (!osGranted) return false

        when (decision) {
            PermissionDecision.AllowAlways -> repository.grant(productId, permission)
            PermissionDecision.AllowOnce -> repository.grantOneTime(productId, permission)
            else -> Unit
        }
        return true
    }

    override suspend fun revoke(productId: ProductId, permission: ProductPermission.DeviceCapability) {
        repository.revoke(productId, permission)
    }

    suspend fun requestOsPermissionIfNeeded(capability: DeviceCapabilityType): Boolean {
        val manifestPermission = capability.toManifestPermission() ?: return true
        val result = permissionAsker.askPermission(manifestPermission)
        return result == PermissionResult.GRANTED
    }

    /**
     * The OS answer for [capability], for a request that can come with no screen, such as from a chat worker.
     * A permission that the OS gave already needs no activity. Otherwise the OS dialog shows only when the app is in
     * front. Returns null when no screen can show the dialog: that is not a refusal.
     */
    suspend fun osPermissionOrNull(capability: DeviceCapabilityType): Boolean? {
        val manifestPermission = capability.toManifestPermission() ?: return true
        if (ContextCompat.checkSelfPermission(context, manifestPermission) == PackageManager.PERMISSION_GRANTED) {
            return true
        }
        return foregroundPrompt.ask { permissionAsker.askPermission(manifestPermission) == PermissionResult.GRANTED }
    }

    private fun DeviceCapabilityType.toManifestPermission(): String? = when (this) {
        DeviceCapabilityType.Camera -> Manifest.permission.CAMERA
        DeviceCapabilityType.Microphone -> Manifest.permission.RECORD_AUDIO
        DeviceCapabilityType.Bluetooth -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            Manifest.permission.BLUETOOTH_CONNECT
        } else {
            null
        }
        DeviceCapabilityType.Location -> Manifest.permission.ACCESS_FINE_LOCATION
        DeviceCapabilityType.Notifications -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            Manifest.permission.POST_NOTIFICATIONS
        } else {
            null
        }
        DeviceCapabilityType.NFC -> Manifest.permission.NFC
        DeviceCapabilityType.Clipboard,
        DeviceCapabilityType.Biometrics,
        DeviceCapabilityType.OpenUrl,
        DeviceCapabilityType.Motion -> null
    }
}
