package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import android.content.Context
import androidx.core.net.toUri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.handleAndProcessOutcomeWithSystemFallback
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import javax.inject.Inject

/**
 * Opens the link of a product action after the user confirmed it (02-DESIGN §3.3.8). The deeplink handlers of the app
 * take the link first, so a link into the app or into a product stays in the app; the system opens any other link.
 */
class ChatProductLinkOpener @Inject constructor(
    private val deepLinkHandler: DeepLinkHandler,
    @param:ApplicationContext private val appContext: Context,
) {
    context(scope: ComputationalScope, messageDisplay: MessageDisplay)
    suspend fun open(url: String) {
        with(appContext) {
            deepLinkHandler.handleAndProcessOutcomeWithSystemFallback(url.toUri())
        }
    }
}
