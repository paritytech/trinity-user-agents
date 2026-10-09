package io.paritytech.polkadotapp.common.utils.permissions

import androidx.activity.ComponentActivity
import androidx.lifecycle.Lifecycle
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.withContext
import javax.inject.Inject

// A backgrounded app may still hold an activity, but a prompt there is never answered. A destroyed
// activity drops the result as well, so the wait ends with it, unanswered.
class ForegroundPrompt @Inject constructor(
    private val contextManager: ContextManager,
    private val appLifecycleObserver: AppLifecycleObserver,
) {
    suspend fun <T> ask(prompt: suspend (ComponentActivity) -> T?): T? {
        if (appLifecycleObserver.getCurrentState() != AppLifecycleState.FOREGROUND) return null
        return withContext(Dispatchers.Main.immediate) {
            val activity = contextManager.getActivity() ?: return@withContext null
            runCancellableCatching {
                coroutineScope {
                    val answer = async { prompt(activity) }
                    val destroyed = launch { activity.lifecycle.currentStateFlow.first { it == Lifecycle.State.DESTROYED } }
                    select<T?> {
                        answer.onAwait { it }
                        destroyed.onJoin { null }
                    }.also { coroutineContext.cancelChildren() }
                }
            }.getOrNull()
        }
    }
}
