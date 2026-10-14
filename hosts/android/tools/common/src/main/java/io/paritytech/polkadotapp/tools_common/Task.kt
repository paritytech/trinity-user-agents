package io.paritytech.polkadotapp.tools_common

import com.google.android.gms.tasks.Task
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

suspend fun <T> Task<T>.executeSuspend(): Result<T> = suspendCancellableCoroutine { continuation ->
    addOnCompleteListener { completion ->
        val exception = completion.exception
        val result = when {
            !continuation.isActive -> return@addOnCompleteListener
            completion.isSuccessful && exception == null -> Result.success(completion.result)
            exception != null -> Result.failure(exception)
            else -> Result.failure(Throwable("Task is completed with failure"))
        }
        continuation.resume(result)
    }
        .addOnCanceledListener {
            if (continuation.isActive) continuation.cancel()
        }
}
