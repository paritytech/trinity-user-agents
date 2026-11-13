package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.FundingResume
import kotlinx.coroutines.launch
import javax.inject.Inject

/** Resumes funding left unfinished at launch. */
class FundingResumeInitializer @Inject constructor(
    private val fundingResume: FundingResume,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch { fundingResume.resumeUnfinished() }
        return Result.success(Unit)
    }
}
