package io.paritytech.polkadotapp.common.utils

import android.content.BroadcastReceiver
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

/**
 * Runs [block] off the main thread after `onReceive` returns. `goAsync()` keeps the receiver
 * (and its process) alive until [block] completes, so the work must finish promptly — the
 * system still imposes a ~10s ceiling.
 */
context(broadcastReceiver: BroadcastReceiver)
fun launchAsyncJob(block: suspend CoroutineScope.() -> Unit) {
    val pending = broadcastReceiver.goAsync()
    CoroutineScope(Dispatchers.IO + SupervisorJob()).launch {
        try {
            block()
        } finally {
            pending.finish()
        }
    }
}
