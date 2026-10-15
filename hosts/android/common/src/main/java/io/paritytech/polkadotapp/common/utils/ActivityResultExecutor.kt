package io.paritytech.polkadotapp.common.utils

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.os.Bundle
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
    protected abstract fun createIntent(): Intent
    protected abstract fun handleResult(result: ActivityResult): Result<T>

    /**
     * Starts the activity of [createIntent] and gives its result.
     *
     * The system can make the activity again while the other activity runs, for example for a configuration
     * change. The result then goes to the registry of the new activity, under the same key. Thus the executor
     * registers the key on each new activity of the same class, until the result comes.
     */
    suspend fun execute(): Result<T> = suspendCancellableCoroutine { continuation ->
        val key = UUID.randomUUID().toString()
        val application = activity.application
        var launcher: ActivityResultLauncher<Intent>? = null
        lateinit var follower: Application.ActivityLifecycleCallbacks

        fun stop() {
            launcher?.unregister()
            launcher = null
            application.unregisterActivityLifecycleCallbacks(follower)
        }

        fun register(on: ComponentActivity) = on.activityResultRegistry.register(
            key,
            ActivityResultContracts.StartActivityForResult()
        ) { result ->
            stop()
            if (continuation.isActive) continuation.resume(handleResult(result))
        }

        follower = object : Application.ActivityLifecycleCallbacks {
            override fun onActivityCreated(created: Activity, savedInstanceState: Bundle?) {
                if (created === activity || created !is ComponentActivity || created.javaClass != activity.javaClass) return
                launcher = register(created)
            }

            override fun onActivityStarted(activity: Activity) = Unit
            override fun onActivityResumed(activity: Activity) = Unit
            override fun onActivityPaused(activity: Activity) = Unit
            override fun onActivityStopped(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) = Unit
        }

        application.registerActivityLifecycleCallbacks(follower)
        continuation.invokeOnCancellation { stop() }

        launcher = register(activity).also { it.launch(createIntent()) }
    }
}
