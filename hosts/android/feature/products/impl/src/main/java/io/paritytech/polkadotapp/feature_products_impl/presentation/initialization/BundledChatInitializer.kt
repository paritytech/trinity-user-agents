package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.ChatProductReleases
import kotlinx.coroutines.launch
import javax.inject.Inject

// Installs the chat product shipped in the APK. It always boots on the core runtime (see
// RealWorkerBootFactory), whose PersonChat it needs.
internal class BundledChatInitializer @Inject constructor(
    private val chatProductReleases: ChatProductReleases,
    private val dotNsTldProvider: DotNsTldProvider,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            runCancellableCatching { chatProductReleases.resolved(dotNsTldProvider.getTldRetrying()) }
                .logFailure("Failed to install the chat product")
        }
        return Result.success(Unit)
    }
}
