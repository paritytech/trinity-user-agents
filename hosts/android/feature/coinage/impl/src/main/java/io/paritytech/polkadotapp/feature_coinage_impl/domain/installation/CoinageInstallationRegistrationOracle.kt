package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageInstallationId
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogW
import io.paritytech.polkadotapp.feature_revive_api.EvmAccountId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.CheckpointBlock
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxEntry
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.MonotoneEffectOracle
import javax.inject.Inject
import javax.inject.Singleton

// Sound as a monotone oracle: the contract only ever appends, only this seed's data store account writes its list,
// and the registrar keeps at most one attempt per target in flight — so a present record is evidence about that
// attempt alone. The contract is read from the attempt's own group, never from config that may have moved since.
@Singleton
class CoinageInstallationRegistrationOracle @Inject constructor(
    private val knownChains: KnownChains,
    private val dataStoreRepository: AccountDataStoreRepository,
) : MonotoneEffectOracle() {
    override val chainId: ChainId
        get() = knownChains.assetHub

    override suspend fun effectsAt(transactions: List<DurableTxEntry>, at: CheckpointBlock): Map<DurableTxId, Boolean> {
        val targets = transactions.mapNotNull { tx -> tx.groupId?.registrationTargetOrNull()?.let { tx.id to it } }
        val registeredByContract = targets.map { (_, target) -> target.contract }
            .distinct()
            .mapNotNull { contract -> registeredIn(contract, at)?.let { contract to it } }
            .toMap()

        return targets.mapNotNull { (id, target) ->
            val registered = registeredByContract[target.contract] ?: return@mapNotNull null

            id to (target.installation in registered)
        }.toMap()
    }

    private suspend fun registeredIn(contract: EvmAccountId, at: CheckpointBlock): Set<CoinageInstallationId>? {
        return dataStoreRepository.fetchRegisteredInstallations(contract, at.blockHash)
            .onFailure { coinageLogW("Installation registration oracle: could not read registered installations at ${at.blockNumber}: ${it.message}") }
            .getOrNull()
    }
}
