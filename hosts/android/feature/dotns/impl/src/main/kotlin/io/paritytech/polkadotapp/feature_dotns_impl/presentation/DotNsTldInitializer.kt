package io.paritytech.polkadotapp.feature_dotns_impl.presentation

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class DotNsTldInitializer @Inject constructor(
    private val dotNsTldProvider: DotNsTldProvider
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        scope.launch { dotNsTldProvider.getTld() }
    }
}
