package io.paritytech.polkadotapp.feature_pgas_api.data.calls

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.domain.model.AccountId

@JvmInline
value class PgasCalls(val extrinsicBuilder: ExtrinsicBuilder)

val ExtrinsicBuilder.pgas: PgasCalls
    get() = PgasCalls(this)

fun PgasCalls.claimPgas(slotIndex: UInt, target: AccountId) {
    extrinsicBuilder.call(
        moduleName = Modules.PGAS,
        callName = "claim_pgas",
        arguments = autoEncodedArgs(
            "slot_index" to slotIndex,
            "target" to target,
        ),
    )
}
