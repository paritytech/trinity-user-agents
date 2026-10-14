package io.paritytech.polkadotapp.common.presentation.deeplink

import android.content.Context
import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.openUri
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import timber.log.Timber

interface DeepLinkHandler {
    suspend fun canHandle(data: Uri): Boolean

    context(scope: ComputationalScope)
    suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome>

    companion object {
        const val APP_SCHEME = "polkadotapp"
        const val WEB_HTTP_SCHEME = "http"
        const val WEB_HTTPS_SCHEME = "https"
    }
}

/** Swaps the scheme rather than prefixing it: ensureHttpsProtocol would mangle a polkadotapp:// deeplink. */
fun Uri.asWebUri(): Uri = buildUpon().scheme(DeepLinkHandler.WEB_HTTPS_SCHEME).build()

sealed class DeeplinkProcessingOutcome {
    class ShowMessage(val message: String) : DeeplinkProcessingOutcome()

    class Navigate(val navigate: () -> Unit) : DeeplinkProcessingOutcome()

    data object NoOp : DeeplinkProcessingOutcome()
}

fun Result<DeeplinkProcessingOutcome>.flatten(): DeeplinkProcessingOutcome {
    return fold(
        onSuccess = { it },
        onFailure = {
            it.message?.let(DeeplinkProcessingOutcome::ShowMessage)
                ?: DeeplinkProcessingOutcome.NoOp
        }
    )
}

suspend fun DeepLinkHandler.tryCanHandle(data: Uri): Boolean {
    return runCatching { canHandle(data) }
        .onFailure { Timber.e("Deeplink canHandle threw an error: $it") }
        .getOrDefault(false)
}

context(scope: ComputationalScope, messageDisplay: MessageDisplay, context: Context)
suspend fun DeepLinkHandler.handleAndProcessOutcomeWithSystemFallback(data: Uri) {
    if (!canHandle(data)) {
        context.openUri(data)
        return
    }

    val outcome = handle(data)
        .onFailure { Timber.e(it, "Failed to handle navigation deeplink: $data") }
        .flatten()

    when (outcome) {
        is DeeplinkProcessingOutcome.Navigate -> withContext(Dispatchers.Main) { outcome.navigate() }
        DeeplinkProcessingOutcome.NoOp -> Unit
        is DeeplinkProcessingOutcome.ShowMessage -> messageDisplay.showMessage(outcome.message)
    }
}
