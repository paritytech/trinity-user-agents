package io.paritytech.polkadotapp.feature_balances_impl.data.updaters

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.requests.SharedRequestsBuilder
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.network.updaters.noSideAffects
import io.paritytech.polkadotapp.chains.util.enabledAssets
import io.paritytech.polkadotapp.common.utils.mergeIfMultiple
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import kotlinx.coroutines.flow.Flow
import timber.log.Timber

internal class BalancesUpdater(
    override val scope: Updater.NoChainScope<MetaAccount>,
    private val balanceTypeRegistry: TokenBalanceTypeRegistry,
) : Updater<MetaAccount> {
    override suspend fun listenForUpdates(scopeValue: MetaAccount, context: Updater.Context): Flow<Updater.SideEffect> {
        @Suppress("UnnecessaryVariable")
        val metaAccount = scopeValue
        val chain = context.chain

        return chain.enabledAssets().mapNotNull { chainAsset ->
            syncAsset(chainAsset, chain, metaAccount, context.storageSubscriptionBuilder)
        }
            .mergeIfMultiple()
            .noSideAffects()
    }

    private suspend fun syncAsset(
        chainAsset: Chain.Asset,
        chain: Chain,
        metaAccount: MetaAccount,
        storageSubscriptionBuilder: SharedRequestsBuilder
    ): Flow<*>? {
        val assetSource = balanceTypeRegistry.typeFor(chainAsset)

        return runCatching {
            assetSource.startSyncingBalance(metaAccount, storageSubscriptionBuilder)
        }
            .onFailure { logSyncError(chain, chainAsset, error = it) }
            .getOrNull()
    }

    private fun logSyncError(chain: Chain, chainAsset: Chain.Asset, error: Throwable) {
        Timber.e(error, "Failed to sync balance for ${chainAsset.symbol} in ${chain.name}")
    }
}
