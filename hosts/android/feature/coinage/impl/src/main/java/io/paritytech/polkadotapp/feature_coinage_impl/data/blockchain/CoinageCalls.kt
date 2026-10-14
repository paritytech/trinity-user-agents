package io.paritytech.polkadotapp.feature_coinage_impl.data.blockchain

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.novasama.substrate_sdk_android.runtime.extrinsic.call
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.common.domain.model.AccountId

@JvmInline
value class CoinageCalls(val extrinsicBuilder: ExtrinsicBuilder)

val ExtrinsicBuilder.coinage: CoinageCalls
    get() = CoinageCalls(this)

fun CoinageCalls.transfer(dest: AccountId) {
    extrinsicBuilder.call(
        moduleName = Modules.COINAGE,
        callName = "transfer",
        arguments = mapOf("to" to dest.value)
    )
}

fun CoinageCalls.loadRecyclerWithCoin(
    memberKey: ByteArray,
    proofOfOwnership: ByteArray
) {
    extrinsicBuilder.call(
        moduleName = Modules.COINAGE,
        callName = "load_recycler_with_coin",
        arguments = mapOf(
            "member_key" to memberKey,
            "proof_of_ownership" to proofOfOwnership
        )
    )
}
