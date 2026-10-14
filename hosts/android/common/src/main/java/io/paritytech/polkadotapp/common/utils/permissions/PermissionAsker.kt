package io.paritytech.polkadotapp.common.utils.permissions

import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import kotlinx.coroutines.suspendCancellableCoroutine
import java.util.UUID
import javax.inject.Inject
import kotlin.coroutines.resume

interface PermissionAsker {
    fun getPermissionState(permission: String): PermissionResult

    suspend fun askPermission(vararg permissions: String): PermissionResult

    fun askPermission(vararg permissions: String, onResult: (PermissionResult) -> Unit)
}

class RealPermissionAsker @Inject constructor(
    private val contextManager: ContextManager,
    private val permissionManager: PermissionStateManager
) : PermissionAsker {
    private var launcher: ActivityResultLauncher<Array<String>>? = null

    override fun getPermissionState(permission: String): PermissionResult {
        return permissionManager.getPermissionState(permission)
    }

    override suspend fun askPermission(vararg permissions: String): PermissionResult =
        suspendCancellableCoroutine { continuation ->
            askPermission(*permissions) {
                if (continuation.isActive) {
                    continuation.resume(it)
                }
            }

            continuation.invokeOnCancellation {
                launcher?.unregister()
                launcher = null
            }
        }

    override fun askPermission(
        vararg permissions: String,
        onResult: (PermissionResult) -> Unit
    ) {
        val activity = contextManager.requireActivity()

        launcher = activity.activityResultRegistry.register(
            UUID.randomUUID().toString(),
            ActivityResultContracts.RequestMultiplePermissions()
        ) { result: Map<String, Boolean> ->
            launcher?.unregister()
            launcher = null

            val finalResult = result.map { (permission, isGranted) ->
                permissionManager.onPermissionResult(
                    permission = permission,
                    isGranted = isGranted
                )
            }

            onResult(finalResult.flattenToMostRelevant())
        }

        launcher?.launch(arrayOf(*permissions))
    }
}

fun PermissionAsker.isPermissionGranted(
    permission: String
): Boolean {
    return getPermissionState(permission) == PermissionResult.GRANTED
}
