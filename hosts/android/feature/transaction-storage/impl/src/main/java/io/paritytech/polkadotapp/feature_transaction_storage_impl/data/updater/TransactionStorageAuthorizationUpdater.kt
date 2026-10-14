package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.updater

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.updaters.SingleStorageKeyUpdater
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.TransactionStorageAuthorizationScope
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.authorizations
import io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain.transactionStorage

internal class TransactionStorageAuthorizationUpdater(
    override val scope: Updater.NoChainScope<MetaAccount>,
    chainRegistry: ChainRegistry,
    storageCache: StorageCache,
) : SingleStorageKeyUpdater<MetaAccount>(
    scope = scope,
    chainRegistry = chainRegistry,
    storageCache = storageCache,
) {
    context(withRuntime: WithRuntime)
    override suspend fun storageKey(scopeValue: MetaAccount, chain: Chain): String {
        val accountId = scopeValue.accountIdIn(chain)
        val authScope = TransactionStorageAuthorizationScope.Account(accountId)
        return withRuntime.runtime.metadata.transactionStorage.authorizations.storageKey(authScope)
    }
}
