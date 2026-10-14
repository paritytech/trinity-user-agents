package io.paritytech.polkadotapp.common.presentation.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.withContext
import javax.inject.Inject

class RootDeeplinkHandler @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
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

    private fun Uri.candidates(): List<Uri> = listOfNotNull(toAppSchemeDeeplinkOrNull(), this)
}
