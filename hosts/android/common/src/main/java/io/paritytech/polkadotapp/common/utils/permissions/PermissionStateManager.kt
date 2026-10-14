package io.paritytech.polkadotapp.common.utils.permissions

import android.content.pm.PackageManager
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.data.storage.preferences.edit
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import javax.inject.Inject

class PermissionStateManager @Inject constructor(
    private val contextManager: ContextManager,
    private val preferences: Preferences
) {
    fun getPermissionState(permission: String): PermissionResult {
        if (ContextCompat.checkSelfPermission(contextManager.applicationContext, permission) == PackageManager.PERMISSION_GRANTED) {
            return PermissionResult.GRANTED
        }

        val shouldShowRationale = ActivityCompat.shouldShowRequestPermissionRationale(contextManager.requireActivity(), permission)
        val wasRationaleShownBefore = isRationaleShown(permission)

        return resolveState(shouldShowRationale, wasRationaleShownBefore)
    }

    fun onPermissionResult(permission: String, isGranted: Boolean): PermissionResult {
        if (isGranted) {
            return PermissionResult.GRANTED
        }

        val shouldShowRationale = ActivityCompat.shouldShowRequestPermissionRationale(contextManager.requireActivity(), permission)

        if (shouldShowRationale) {
            setRationaleShown(permission)
        }

        val wasRationaleShownBefore = isRationaleShown(permission)
        return resolveState(shouldShowRationale, wasRationaleShownBefore)
    }

    private fun resolveState(shouldShowRationale: Boolean, wasRationaleShownBefore: Boolean): PermissionResult {
        return when {
            shouldShowRationale -> PermissionResult.DENIED

            wasRationaleShownBefore -> PermissionResult.DENIED_FOREVER

            else -> PermissionResult.DENIED
        }
    }

    private fun setRationaleShown(permission: String) {
        preferences.edit { putBoolean(getKey(permission), true) }
    }

    private fun isRationaleShown(permission: String): Boolean {
        return preferences.getBoolean(getKey(permission), false)
    }

    private fun getKey(permission: String) = "permission_rationale_shown ($permission)"
}
