package io.paritytech.polkadotapp.feature_transaction_storage_api.data.calls

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.call
import io.paritytech.polkadotapp.chains.util.Modules

@JvmInline
value class TransactionStorageCalls(val extrinsicBuilder: ExtrinsicBuilder)

val ExtrinsicBuilder.transactionStorage: TransactionStorageCalls
    get() = TransactionStorageCalls(this)

fun TransactionStorageCalls.store(data: ByteArray) {
    extrinsicBuilder.call(
        moduleName = Modules.TRANSACTION_STORAGE,
        callName = "store",
        arguments = mapOf(
            "data" to data
        )
    )
}
