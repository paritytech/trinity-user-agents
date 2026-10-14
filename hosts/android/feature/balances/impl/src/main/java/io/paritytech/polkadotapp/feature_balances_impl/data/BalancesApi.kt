package io.paritytech.polkadotapp.feature_balances_impl.data

import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.WithRuntime
import io.paritytech.polkadotapp.chains.util.balances
import io.paritytech.polkadotapp.feature_balances_api.domain.model.BalanceHold
import io.paritytech.polkadotapp.feature_balances_impl.data.bindings.bindBalanceHolds

@JvmInline
value class BalancesApi(override val module: Module) : QueryableModule

val RuntimeMetadata.balances: BalancesApi
    get() = BalancesApi(balances())

context(withRuntime: WithRuntime)
val BalancesApi.holds: QueryableStorageEntry1<ByteArray, List<BalanceHold>>
    get() = storage1("Holds", binding = { dynamic, _ -> bindBalanceHolds(dynamic) })
