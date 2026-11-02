package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import android.content.Context
import android.net.Uri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.handleAndProcessOutcomeWithSystemFallback
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject

/**
 * Where a chat worker's `navigateTo` goes, on either runtime: through the app's deeplink handler,
 * and only while the person is looking at that product's chat. The script keeps running while the
 * chat is closed, so a background bot could otherwise pull them off any screen.
 */
class ChatWorkerNavigation @Inject constructor(
    private val deepLinkHandler: DeepLinkHandler,
    private val chatActiveTracker: ChatActiveTracker,
    private val appNotifier: AppNotifier,
    @param:ApplicationContext private val context: Context,
) {
    // App-level message surface for deeplink fallbacks: a worker has no UI of its own.
    private val messageDisplay = object : MessageDisplay {
        override fun showMessage(text: String) = appNotifier.error(text)
    }

    /** The policy the worker of [productId] navigates through; the deeplink runs on [scope]. */
    fun policy(productId: ProductId, scope: CoroutineScope) = NavigationPolicy.DeeplinkNavigation(
        onDeeplinkNavigation = { destination -> navigate(productId, destination, scope) },
    )

    private fun navigate(productId: ProductId, destination: Uri, scope: CoroutineScope) {
        if (!productChatIsActive(productId)) {
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

    private fun productChatIsActive(productId: ProductId): Boolean {
        val activeChatId = chatActiveTracker.getActive() ?: return false
        return activeChatId.isExtensionChat(productId.toChatExtensionId())
    }
}
