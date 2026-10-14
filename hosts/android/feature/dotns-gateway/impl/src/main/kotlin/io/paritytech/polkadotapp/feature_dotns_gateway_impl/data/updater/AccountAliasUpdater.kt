package io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.updater

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.updaters.SingleStorageKeyUpdater
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.api.accountAlias
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.api.dotNsGateway

class AccountAliasUpdater(
    chainRegistry: ChainRegistry,
    storageCache: StorageCache,
    scope: Updater.NoChainScope<MetaAccount>
) : SingleStorageKeyUpdater<MetaAccount>(scope, chainRegistry, storageCache) {
    context(withRuntime: WithRuntime)
    override suspend fun storageKey(scopeValue: MetaAccount, chain: Chain): String {
        val accountId = scopeValue.accountIdIn(chain)

        return withRuntime.runtime.metadata.dotNsGateway.accountAlias.storageKey(accountId)
    }
}
