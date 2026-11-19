package io.paritytech.polkadotapp.common.utils.permissions

import androidx.core.app.ActivityCompat
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import javax.inject.Inject

interface PermissionStatusChecker {
    fun isGranted(permission: String): Boolean

    fun shouldShowRationale(permission: String): Boolean
}

class RealPermissionStatusChecker @Inject constructor(
    private val contextManager: ContextManager
) : PermissionStatusChecker {
    override fun isGranted(permission: String): Boolean {
        return contextManager.applicationContext.hasPermission(permission)
    }

    override fun shouldShowRationale(permission: String): Boolean {
        return ActivityCompat.shouldShowRequestPermissionRationale(contextManager.requireActivity(), permission)
    }
}
