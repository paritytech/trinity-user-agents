package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.blockchain

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.transactionStorage
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.TransactionStorageAuthorization
import io.paritytech.polkadotapp.feature_transaction_storage_api.domain.model.TransactionStorageAuthorizationScope

@JvmInline
value class TransactionStorageApi(override val module: Module) : QueryableModule

val RuntimeMetadata.transactionStorage: TransactionStorageApi
    get() = TransactionStorageApi(transactionStorage())

context(withRuntime: WithRuntime)
val TransactionStorageApi.authorizations: QueryableStorageEntry1<TransactionStorageAuthorizationScope, TransactionStorageAuthorization>
    get() = storage1("Authorizations")
