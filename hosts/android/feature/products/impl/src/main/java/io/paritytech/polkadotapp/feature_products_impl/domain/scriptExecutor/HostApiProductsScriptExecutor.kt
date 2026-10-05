package io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor

import android.content.Context
import android.net.Uri
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.qualifiers.ApplicationContext
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.util.scaleEncodeBinary
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.handleAndProcessOutcomeWithSystemFallback
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.common.utils.awaitTrue
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_products_api.model.JsUiEvent
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.ExplicitInjection
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiEnvironment
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiSession
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostCallGroupFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups.ChatRenderWidgetHostCalls
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups.HostCallHandlerGroup
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.toJsStringLiteral
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductScriptResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.serialization.JsWidgetSerializer
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ChatWebViewProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import timber.log.Timber

class HostApiProductsScriptExecutor @AssistedInject constructor(
    private val serializer: JsWidgetSerializer,
    private val scriptResolver: ProductScriptResolver,
    private val hostCallGroupFactory: HostCallGroupFactory,
    private val sessionFactory: HostApiSession.Factory,
    private val chatWebViewProviderFactory: ChatWebViewProvider.Factory,
    private val deepLinkHandler: DeepLinkHandler,
    private val chatActiveTracker: ChatActiveTracker,
    private val appNotifier: AppNotifier,
    @param:ApplicationContext private val context: Context,
    @Assisted private val productId: ProductId,
) : ProductsScriptExecutor {
    @AssistedFactory
    interface Factory {
        fun create(productId: ProductId): HostApiProductsScriptExecutor
    }

    private val mutex = Mutex()

    private val chatRenderWidgetHostCalls = ChatRenderWidgetHostCalls()

    private var session: HostApiSession? = null
    private var scope: CoroutineScope? = null
    private val initialized = MutableStateFlow(false)

    override suspend fun initializeBot(botApi: ProductsBotApi, scope: CoroutineScope): Result<Unit> {
        this.scope = scope

        return scriptResolver.resolveWorker(productId).mapCatching { worker ->
            initializeWith(WorkerScript.of(worker.scriptUrl), botApi, scope)
        }
    }

    private suspend fun initializeWith(workerScript: WorkerScript, botApi: ProductsBotApi, scope: CoroutineScope) {
        mutex.withLock {
            if (initialized.value) return@withLock

            val webViewProvider = chatWebViewProviderFactory.create(
                config = ChatWebViewConfig(productId = productId, workerScript = workerScript),
                scope = scope,
            )
            val webViewRuntime = WebViewRuntime(webViewProvider)

            val transport = webViewRuntime.createTransport()
            val navigationPolicy = NavigationPolicy.DeeplinkNavigation(
                onDeeplinkNavigation = { launchDeeplinkNavigation(it, scope) }
            )

            val sharedGroups = hostCallGroupFactory.createShared(botApi, webViewProvider.callingProductIdProvider, navigationPolicy)
            val chatGroup = hostCallGroupFactory.createChatGroup(botApi)
            val allGroups: List<HostCallHandlerGroup> = sharedGroups + chatGroup + chatRenderWidgetHostCalls

            val environment = HostApiEnvironment(
                injectionStrategy = ExplicitInjection(),
                handlerGroups = allGroups,
            )

            val hostApiSession = sessionFactory.create(environment, webViewRuntime, transport, scope)
            hostApiSession.initialize()

            // Bridge is injected by ExplicitInjection during initialize(); load the worker entry
            // module afterwards so its archive-served imports run against an existing bridge.
            hostApiSession.loadEntryModule(workerScript.entrypoint)
                .logFailure("Failed to load worker entry module for product: $productId")

            this.session = hostApiSession
            initialized.value = true

            Timber.d("Initialized HostApi script executor for product: $productId")
        }
    }

    override suspend fun onUserMessage(roomId: String, text: String): Result<Unit> = runCatching {
        awaitInitialized()
        val textLiteral = text.toJsStringLiteral()
        session!!.evaluateScript("dispatchUserMessage(${roomId.toJsStringLiteral()}, $textLiteral)")
            .onFailure { Timber.e(it, "Failed to call onUserMessage for product: $productId") }
    }

    override fun renderMessage(
        roomId: String,
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Flow<Result<JsWidget>> {
        return flow {
            startRendering(messageId, messageType, messageData)
                .onFailure { emit(Result.failure(it)); return@flow }

            val renderingUpdates = chatRenderWidgetHostCalls.renderUpdatesForMessage(messageId)
                .map { update -> serializer.deserialize(update) }
            emitAll(renderingUpdates)
        }.onCompletion {
            chatRenderWidgetHostCalls.removeMessage(messageId)
        }
    }

    override fun dispatchEvent(event: JsUiEvent) {
        scope?.launch {
            try {
                awaitInitialized()
                val messageIdLiteral = event.messageId.toJsLiteral()
                val actionIdLiteral = event.actionId.toJsLiteral()
                val payloadLiteral = encodePayload(event).toJsLiteral()
                session!!.evaluateScript("dispatchChatAction('', $messageIdLiteral, $actionIdLiteral, $payloadLiteral)")
            } catch (e: Exception) {
                Timber.e(e, "Failed to dispatch event: ${event.actionId}")
            }
        }
    }

    private suspend fun awaitInitialized() {
        initialized.awaitTrue()
    }

    // App-level message surface for deeplink fallbacks (the chat executor has no UI of its own).
    private val messageDisplay = object : MessageDisplay {
        override fun showMessage(text: String) = appNotifier.error(text)
    }

    // The script keeps running while the chat is closed, so navigation is only honoured when the user
    // is actually looking at this product's chat - otherwise a background bot could yank them off any screen.
    private fun productChatIsActive(): Boolean {
        val activeChatId = chatActiveTracker.getActive() ?: return false
        return activeChatId.isExtensionChat(productId.toChatExtensionId())
    }

    private fun launchDeeplinkNavigation(destination: Uri, scope: CoroutineScope) {
        if (!productChatIsActive()) {
            Timber.d("Ignored navigation to $destination: chat of $productId is not active")
            return
        }

        scope.launch {
            with(ComputationalScope(this)) {
                with(messageDisplay) {
                    with(context) {
                        deepLinkHandler.handleAndProcessOutcomeWithSystemFallback(destination)
                    }
                }
            }
        }
    }

    private suspend fun startRendering(
        messageId: ChatMessageId,
        messageType: String,
        messageData: DataByteArray,
    ): Result<Unit> = runCatching {
        awaitInitialized()
        val js = buildInitiateRenderingJs(messageType, messageData, messageId)
        session!!.evaluateScript(js)
    }

    private fun buildInitiateRenderingJs(
        messageType: String,
        data: DataByteArray,
        messageId: ChatMessageId,
    ): String {
        val hexData = data.value.toHexString(withPrefix = true)
        val escapedMessageId = messageId.replace("\"", "\\\"")
        val escapedMessageType = messageType.replace("\"", "\\\"")

        return """
            (function() {
                try {
                    if (typeof window.renderMessage === 'function') {
                        window.renderMessage("$escapedMessageType", "$hexData", "$escapedMessageId");
                    }
                } catch (e) {
                    console.error('renderMessage error:', e);
                }
            })();
        """.trimIndent()
    }

    private fun encodePayload(event: JsUiEvent): HexString? {
        return when (val type = event.eventType) {
            JsUiEvent.Type.ButtonClick -> null
            is JsUiEvent.Type.InputFieldValueChange -> type.newValue.scaleEncodeBinary()
                .toHexString(withPrefix = true)
        }
    }

    // `undefined` for null, otherwise a quoted JS string literal.
    private fun String?.toJsLiteral(): String = this?.toJsStringLiteral() ?: "undefined"
}
