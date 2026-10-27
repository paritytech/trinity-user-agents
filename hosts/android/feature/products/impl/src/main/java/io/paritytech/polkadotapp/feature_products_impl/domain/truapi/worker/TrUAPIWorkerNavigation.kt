package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

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

class TrUAPIWorkerNavigation @Inject constructor(
    private val deepLinkHandler: DeepLinkHandler,
    private val chatActiveTracker: ChatActiveTracker,
    private val appNotifier: AppNotifier,
    @param:ApplicationContext private val context: Context,
) {
    private val messageDisplay = object : MessageDisplay {
        override fun showMessage(text: String) = appNotifier.error(text)
    }

    fun policy(productId: ProductId, scope: CoroutineScope): NavigationPolicy = NavigationPolicy.DeeplinkNavigation { destination ->
        // The worker keeps running after its chat closes, and a navigation it sent then would pull the user off
        // whatever screen they are on.
        if (!chatIsOnScreen(productId)) {
            Timber.w("Refused navigation from the worker of %s to %s: its chat is not on screen", productId.value, destination)
            throw IllegalStateException("The chat of ${productId.value} is not on screen")
        }
        scope.launchDeeplinkNavigation(destination)
    }

    private fun chatIsOnScreen(productId: ProductId): Boolean =
        chatActiveTracker.getActive()?.isExtensionChat(productId.toChatExtensionId()) == true

    private fun CoroutineScope.launchDeeplinkNavigation(destination: Uri) {
        launch {
            with(ComputationalScope(this)) {
                with(messageDisplay) {
                    with(context) {
                        deepLinkHandler.handleAndProcessOutcomeWithSystemFallback(destination)
                    }
                }
            }
        }
    }
}
