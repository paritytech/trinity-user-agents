package io.paritytech.polkadotapp.feature_balances_impl.data.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.multiNetwork.requests.subscribe
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.network.updaters.system.UpdateSystem
import io.paritytech.polkadotapp.chains.util.isDisabled
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.mergeIfMultiple
import io.paritytech.polkadotapp.common.utils.transformLatestDiffed
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import timber.log.Timber
import kotlin.coroutines.coroutineContext

class BalancesUpdateSystem(
    private val chainRegistry: ChainRegistry,
    private val accountBalancesUpdater: Updater<MetaAccount>,
    private val accountUpdateScope: Updater.NoChainScope<MetaAccount>,
    private val storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    private val coroutineDispatchers: CoroutineDispatchers
) : UpdateSystem {
    override fun start(): Flow<Updater.SideEffect> {
        return accountUpdateScope.invalidationFlow().flatMapLatest { metaAccount ->
            chainRegistry.currentChains.transformLatestDiffed { chain ->
                emitAll(balancesSync(chain, metaAccount))
            }
        }.flowOn(coroutineDispatchers.io)
    }

    private fun balancesSync(chain: Chain, metaAccount: MetaAccount): Flow<Updater.SideEffect> {
        return when {
            chain.connectionState.isDisabled -> emptyFlow()
            else -> fullBalancesSync(chain, metaAccount)
        }
    }

    private fun fullBalancesSync(chain: Chain, metaAccount: MetaAccount): Flow<Updater.SideEffect> {
        return flow {
            val subscriptionBuilder = storageSharedRequestsBuilderFactory.create(chain.id)
            val updaters = createFullSyncUpdaters()
            val context = Updater.Context(subscriptionBuilder, chain)

            val sideEffectFlows = updaters.map { updater ->
                try {
                    updater.listenForUpdates(metaAccount, context)
                        .catch { logError(chain, it) }
                } catch (e: Exception) {
                    emptyFlow()
                }
            }

            subscriptionBuilder.subscribe(coroutineContext)
            val resultFlow = sideEffectFlows.mergeIfMultiple()

            emitAll(resultFlow)
        }.catch { logError(chain, it) }
    }

    private fun createFullSyncUpdaters(): List<Updater<MetaAccount>> {
        return listOf(accountBalancesUpdater)
    }

    private fun logError(chain: Chain, error: Throwable) {
        Timber.e(error, "Failed to subscribe to balances in ${chain.name}: ${error.message}")
    }
}
