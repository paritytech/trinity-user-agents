package io.paritytech.polkadotapp.common.utils

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlin.coroutines.CoroutineContext
import kotlin.coroutines.EmptyCoroutineContext

inline fun CoroutineScope.invokeOnCompletion(crossinline action: () -> Unit) {
    coroutineContext[Job]?.invokeOnCompletion { action() }
}

fun CoroutineScope.childScope(supervised: Boolean = true): CoroutineScope {
    val parentJob = coroutineContext[Job]

    val job = if (supervised) SupervisorJob(parent = parentJob) else Job(parent = parentJob)

    return CoroutineScope(coroutineContext + job)
}

inline fun <T> CoroutineScope.lazyAsync(context: CoroutineContext = EmptyCoroutineContext, crossinline producer: suspend () -> T) = lazy {
    async(context) { producer() }
}

fun CoroutineScope.launchUnit(
    context: CoroutineContext = EmptyCoroutineContext,
    start: CoroutineStart = CoroutineStart.DEFAULT,
    block: suspend CoroutineScope.() -> Unit
) { launch(context, start, block) }
