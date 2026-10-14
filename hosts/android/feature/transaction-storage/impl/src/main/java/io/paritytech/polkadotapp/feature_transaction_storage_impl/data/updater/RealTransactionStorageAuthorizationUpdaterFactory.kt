package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.updater

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.TransactionStorageAuthorizationUpdaterFactory
import javax.inject.Inject

class RealTransactionStorageAuthorizationUpdaterFactory @Inject constructor(
    private val chainRegistry: ChainRegistry,
    private val storageCache: StorageCache,
) : TransactionStorageAuthorizationUpdaterFactory {
    override fun create(scope: Updater.NoChainScope<MetaAccount>): Updater<MetaAccount> {
        return TransactionStorageAuthorizationUpdater(
            scope = scope,
            chainRegistry = chainRegistry,
            storageCache = storageCache,
        )
    }
}
