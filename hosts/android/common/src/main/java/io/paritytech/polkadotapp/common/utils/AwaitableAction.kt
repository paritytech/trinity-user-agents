package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlin.coroutines.resume

class AwaitableAction<P, R>(
    val payload: P,
    val onSuccess: (R) -> Unit,
    val onCancel: () -> Unit,
)

typealias AwaitableActionChannel<P, R> = Channel<AwaitableAction<P, R>>
typealias AwaitableActionFlow<P, R> = Flow<AwaitableAction<P, R>>

fun <P, R> AwaitableActionChannel(): AwaitableActionChannel<P, R> = OneShotEventChannel()

suspend fun <P, R> AwaitableActionChannel<P, R>.awaitAction(payload: P): R = suspendCancellableCoroutine { continuation ->
    val action = AwaitableAction<P, R>(
        payload = payload,
        onSuccess = { continuation.resume(it) },
        onCancel = { continuation.cancel() }
    )

    trySend(action)
}
