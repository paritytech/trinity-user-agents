package io.paritytech.polkadotapp.feature_transaction_storage_api.domain

import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount

/**
 * Builds a runtime updater that keeps the local cache in sync with the on-chain
 * `Authorizations` entry for the given [scope]'s account on the Bullet-In chain.
 */
interface TransactionStorageAuthorizationUpdaterFactory {
    fun create(scope: Updater.NoChainScope<MetaAccount>): Updater<MetaAccount>
}
