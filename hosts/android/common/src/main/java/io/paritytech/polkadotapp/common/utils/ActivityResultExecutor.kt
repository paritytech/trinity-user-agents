package io.paritytech.polkadotapp.common.utils

import android.content.Intent
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResult
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import kotlinx.coroutines.suspendCancellableCoroutine
import java.util.UUID
import kotlin.coroutines.resume

abstract class ActivityResultExecutor<T>(
    private val activity: ComponentActivity
) {
    private var launcher: ActivityResultLauncher<Intent>? = null

    protected abstract fun createIntent(): Intent
    protected abstract fun handleResult(result: ActivityResult): Result<T>

    suspend fun execute() = suspendCancellableCoroutine { continuation ->
        launcher = activity.activityResultRegistry.register(
            UUID.randomUUID().toString(),
            ActivityResultContracts.StartActivityForResult()
        ) { result ->
            launcher?.unregister()
            launcher = null

            continuation.resume(handleResult(result))

            continuation.invokeOnCancellation {
                launcher?.unregister()
                launcher = null
            }
        }

        launcher?.launch(createIntent())
    }
}
