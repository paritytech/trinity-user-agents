package io.paritytech.polkadotapp.common.utils.permissions

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import javax.inject.Inject

class PermissionStateManager @Inject constructor(
    private val permissionStatusChecker: PermissionStatusChecker,
    private val preferences: Preferences
) {
    fun getPermissionState(permission: String): PermissionResult {
        if (permissionStatusChecker.isGranted(permission)) {
            return PermissionResult.GRANTED
        }

        return resolveDenial(permissionStatusChecker.shouldShowRationale(permission), wasDeniedBefore(permission))
    }

    fun onPermissionResult(permission: String, isGranted: Boolean): PermissionResult {
        if (isGranted) {
            setDeniedBefore(permission, false)
            return PermissionResult.GRANTED
        }

        val result = resolveDenial(permissionStatusChecker.shouldShowRationale(permission), wasDeniedBefore(permission))
        setDeniedBefore(permission, true)
        return result
    }

    // shouldShowRationale is false whether the permission was never asked, dismissed, or denied for good.
    private fun resolveDenial(shouldShowRationale: Boolean, wasDeniedBefore: Boolean): PermissionResult {
        return when {
            shouldShowRationale -> PermissionResult.DENIED

            wasDeniedBefore -> PermissionResult.DENIED_FOREVER

            else -> PermissionResult.DENIED
        }
    }

    private fun setDeniedBefore(permission: String, denied: Boolean) {
        preferences.putBoolean(deniedKey(permission), denied)
    }

    private fun wasDeniedBefore(permission: String): Boolean {
        return preferences.getBoolean(deniedKey(permission), preferences.getBoolean(rationaleShownKey(permission), false))
    }

    private fun deniedKey(permission: String) = "permission_denied ($permission)"

    private fun rationaleShownKey(permission: String) = "permission_rationale_shown ($permission)"
}
