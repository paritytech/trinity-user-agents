package io.paritytech.polkadotapp.app.root.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.DeeplinkProcessingOutcome
import io.paritytech.polkadotapp.common.presentation.deeplink.tryCanHandle
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import kotlinx.coroutines.withContext
import javax.inject.Inject

internal class RootDeeplinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val dotNsTldProvider: DotNsTldProvider,
    private val handlers: Set<@JvmSuppressWildcards DeepLinkHandler>,
) : DeepLinkHandler {
    override suspend fun canHandle(data: Uri): Boolean {
        return data.candidates().any { uri -> handlers.any { it.canHandle(uri) } }
    }

    context(scope: ComputationalScope)
    override suspend fun handle(data: Uri): Result<DeeplinkProcessingOutcome> = withContext(coroutineDispatchers.computation) {
        data.candidates()
            .firstNotNullOfOrNull { uri ->
                handlers.find { it.tryCanHandle(uri) }
                    ?.handle(uri)
            }
            ?: Result.failure(IllegalArgumentException("No matching handlers found to handle deeplink: $data"))
    }

    // Which share domains are this app's depends on the network, so none is rewritten before its TLD is known.
    private fun Uri.candidates(): List<Uri> =
        listOfNotNull(dotNsTldProvider.currentTldOrNull()?.let { toAppSchemeDeeplinkOrNull(it) }, this)
}
