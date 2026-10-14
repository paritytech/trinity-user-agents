package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_products_impl.domain.topUpRequest.TopUpService
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Picks up the top-ups a previous run left unfinished.
 *
 * Resuming only when a product asks after one would leave money sitting on a key nobody is watching, for as
 * long as the product goes unopened — and the retry window would expire meanwhile, turning a top-up that was
 * merely interrupted into one that terminally failed.
 */
class TopUpResumeInitializer @Inject constructor(
    private val topUpService: TopUpService,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            runCancellableCatching { topUpService.resumeUnfinished() }
                .logFailure("Failed to resume unfinished top-ups")
        }

        return Result.success(Unit)
    }
}
