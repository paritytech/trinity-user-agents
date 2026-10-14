package io.paritytech.polkadotapp.feature_transactions.api.data.origins

import io.novasama.substrate_sdk_android.runtime.RuntimeSnapshot
import io.novasama.substrate_sdk_android.runtime.definitions.types.composite.DictEnum
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.InheritedImplication
import io.novasama.substrate_sdk_android.runtime.extrinsic.v5.transactionExtension.TransactionExtension
import io.paritytech.polkadotapp.chains.util.findNonceOrThrow

class PeopleLiteAuth : TransactionExtension {
    override val name: String = "PeopleLiteAuth"

    override suspend fun explicit(
        inheritedImplication: InheritedImplication,
        runtimeSnapshot: RuntimeSnapshot
    ): Any {
        val nonce = inheritedImplication.findNonceOrThrow()

        return DictEnum.Entry(
            name = "AsLitePerson",
            value = nonce
        )
    }

    override suspend fun implicit(): Any? = null
}
