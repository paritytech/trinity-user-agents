package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.net.Uri
import android.webkit.ConsoleMessage
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.view.View
import android.webkit.WebView
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import timber.log.Timber

class ProductWebChromeClient @AssistedInject constructor(
    private val contextManager: ContextManager,
    private val fileProvider: FileProvider,
    @Assisted private val logPrefix: String,
    @Assisted private val callingProductIdProvider: CallingProductIdProvider,
    @Assisted private val scope: CoroutineScope,
    @Assisted private val onTitleReceived: ((String) -> Unit)?,
) : WebChromeClient() {
    @AssistedFactory
    interface Factory {
        fun create(
            logPrefix: String,
            callingProductIdProvider: CallingProductIdProvider,
            scope: CoroutineScope,
            onTitleReceived: ((String) -> Unit)?,
        ): ProductWebChromeClient
    }

    override fun onConsoleMessage(consoleMessage: ConsoleMessage?): Boolean {
        consoleMessage?.let {
            // sourceId names the frame the message came from. Without it a violation raised inside an
            // embedded third-party iframe is indistinguishable from one in the product's own document.
            val source = it.sourceId().orEmpty().ifEmpty { UNKNOWN_CONSOLE_SOURCE }
            val message = "$logPrefix: ${it.message()} [$source:${it.lineNumber()}]"
            it.messageLevel().timberLog(message)
        }
        return true
    }

    override fun onReceivedTitle(view: WebView?, title: String?) {
        if (!title.isNullOrEmpty()) {
            onTitleReceived?.invoke(title)
        }
    }

    override fun onPermissionRequest(request: PermissionRequest) {
        // Product realms never receive raw OS camera/microphone streams. The
        // generated Media service has its own trusted, cancellable consent.
        request.deny()
    }

    override fun onShowCustomView(view: View?, callback: CustomViewCallback?) {
        callback?.onCustomViewHidden()
    }

    override fun onShowFileChooser(
        webView: WebView?,
        filePathCallback: ValueCallback<Array<Uri>>?,
        fileChooserParams: FileChooserParams?,
    ): Boolean {
        // Without this override WebView silently no-ops <input type="file">.
        if (filePathCallback == null || fileChooserParams == null) return false

        scope.launch {
            // The callback must be invoked exactly once (even with null) or the <input> stays locked
            // and never reopens. The user picking a file is the consent, so no permission gate here —
            // matching the iOS/desktop clients.
            val uris = runCatching {
                WebFileChooserExecutor(contextManager.requireActivity(), fileChooserParams, fileProvider).execute()
            }.flatMap { it }
                .logFailure("$logPrefix: file chooser failed")
                .getOrNull()
            filePathCallback.onReceiveValue(uris)
        }
        return true
    }

    private companion object {
        const val UNKNOWN_CONSOLE_SOURCE = "unknown"
    }
}


private fun ConsoleMessage.MessageLevel?.timberLog(message: String) {
    when (this) {
        ConsoleMessage.MessageLevel.TIP -> Timber.v(message)
        ConsoleMessage.MessageLevel.LOG -> Timber.d(message)
        ConsoleMessage.MessageLevel.WARNING -> Timber.w(message)
        ConsoleMessage.MessageLevel.ERROR -> Timber.e(message)
        ConsoleMessage.MessageLevel.DEBUG -> Timber.d(message)
        null -> Timber.d(message)
    }
}
