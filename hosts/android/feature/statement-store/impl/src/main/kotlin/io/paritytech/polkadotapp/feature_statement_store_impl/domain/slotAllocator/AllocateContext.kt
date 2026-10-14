package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_people_api.domain.useCase.ActivePeopleCollectionUseCase
import javax.inject.Inject

data class AllocateContext(
    val chain: Chain,
    val availableCollections: List<PeopleCollection>,
    val period: UInt,
)

class AllocateContextResolver @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val activePeopleCollectionUseCase: ActivePeopleCollectionUseCase,
    private val currentPeriodProvider: CurrentPeriodProvider,
) {
    suspend fun resolve(): Result<AllocateContext> = runCatching {
        val chain = chainRegistry.getChain(knownChains.people)
        val availableCollections = activePeopleCollectionUseCase.getAvailableCollections()
        val period = currentPeriodProvider.current()
        AllocateContext(chain, availableCollections, period)
    }
}
