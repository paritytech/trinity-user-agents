@file:Suppress("RedundantUnitExpression")

package io.paritytech.polkadotapp.tools_hydration_sdk_impl.fee.api

import io.novasama.substrate_sdk_android.runtime.AccountId
import io.novasama.substrate_sdk_android.runtime.metadata.RuntimeMetadata
import io.novasama.substrate_sdk_android.runtime.metadata.module.Module
import io.paritytech.polkadotapp.chains.network.binding.bindPerquintill
import io.paritytech.polkadotapp.chains.storage.source.query.StorageQueryContext
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableModule
import io.paritytech.polkadotapp.chains.storage.source.query.api.QueryableStorageEntry1
import io.paritytech.polkadotapp.chains.storage.source.query.api.storage1
import io.paritytech.polkadotapp.chains.util.multiTransactionPayment
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.tools_hydration_sdk_impl.assets.HydraDxAssetId

@JvmInline
value class MultiTransactionPaymentApi(override val module: Module) : QueryableModule

val RuntimeMetadata.multiTransactionPayment: MultiTransactionPaymentApi
    get() = MultiTransactionPaymentApi(multiTransactionPayment())

context(storage: StorageQueryContext)
val MultiTransactionPaymentApi.acceptedCurrencies: QueryableStorageEntry1<HydraDxAssetId, Fraction>
    get() = storage1(
        name = "AcceptedCurrencies",
        binding = { decoded, _ -> bindPerquintill(decoded) }
    )

context(storage: StorageQueryContext)
val MultiTransactionPaymentApi.accountCurrencyMap: QueryableStorageEntry1<AccountId, HydraDxAssetId>
    get() = storage1(
        name = "AccountCurrencyMap",
    )
