package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.net.Uri
import android.webkit.ConsoleMessage
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebView
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.data.storage.file.FileProvider
import io.paritytech.polkadotapp.common.presentation.resources.ContextManager
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.getProductIdOrNull
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.DeviceCapabilityPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.DeviceCapabilityType
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import timber.log.Timber

class ProductWebChromeClient @AssistedInject constructor(
    private val permissionGuard: ProductPermissionGuard,
    private val devicePermissionHandler: DeviceCapabilityPermissionHandler,
    private val contextManager: ContextManager,
    private val fileProvider: FileProvider,
    @Assisted private val logPrefix: String,
    @Assisted private val callingProductIdProvider: CallingProductIdProvider,
    @Assisted private val scope: CoroutineScope,
    @Assisted private val onTitleReceived: ((String) -> Unit)?,
) : WebChromeClient() {
    private var containerPermissions = false

    @AssistedFactory
    interface Factory {
        fun create(
            logPrefix: String,
            callingProductIdProvider: CallingProductIdProvider,
            scope: CoroutineScope,
            onTitleReceived: ((String) -> Unit)?,
        ): ProductWebChromeClient
    }

    fun useContainerPermissions() {
        containerPermissions = true
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
        scope.launch {
            val productId = callingProductIdProvider.getProductIdOrNull()
            if (productId == null) {
                Timber.w("$logPrefix: denying PermissionRequest — no calling product")
                request.deny()
                return@launch
            }

            val grantedResources = request.resources.filter { resource ->
                val capability = resource.toDeviceCapability()
                if (capability == null) {
                    Timber.w("$logPrefix: skipping unsupported PermissionRequest resource '$resource'")
                    return@filter false
                }
                consumeCapability(productId, capability)
            }

            if (grantedResources.isNotEmpty()) {
                request.grant(grantedResources.toTypedArray())
            } else {
                request.deny()
            }
        }
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

    private suspend fun consumeCapability(productId: ProductId, capability: DeviceCapabilityType): Boolean {
        if (containerPermissions) return devicePermissionHandler.requestOsPermissionIfNeeded(capability)
        val permission = ProductPermission.DeviceCapability(capability)
        return permissionGuard.consumePermission(productId, permission)
    }

    private companion object {
        const val UNKNOWN_CONSOLE_SOURCE = "unknown"
    }
}

private fun String.toDeviceCapability(): DeviceCapabilityType? = when (this) {
    PermissionRequest.RESOURCE_VIDEO_CAPTURE -> DeviceCapabilityType.Camera
    PermissionRequest.RESOURCE_AUDIO_CAPTURE -> DeviceCapabilityType.Microphone
    else -> null
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
