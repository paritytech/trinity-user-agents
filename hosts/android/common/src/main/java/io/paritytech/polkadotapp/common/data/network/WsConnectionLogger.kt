
package io.paritytech.polkadotapp.common.data.network

import io.novasama.substrate_sdk_android.wsrpc.logging.Logger
import timber.log.Timber

private const val TRUNCATION_SIZE = 1024

class WsConnectionLogger(private val ignoreStateMachineLogs: Boolean) : Logger {
    override fun log(message: String?) {
        if (message == null) return

        if (shouldIgnoreStateMachineLog(message)) return

        if (message.length > TRUNCATION_SIZE) {
            val message = "${message.take(TRUNCATION_SIZE)}... (${message.length - TRUNCATION_SIZE} more)"
            Timber.d(message)
        } else {
            Timber.d(message)
        }
    }

    override fun log(throwable: Throwable?) {
        Timber.d(throwable)
    }

    private fun shouldIgnoreStateMachineLog(message: String): Boolean {
        return ignoreStateMachineLogs && message.startsWith("[STATE MACHINE]")
    }
}
