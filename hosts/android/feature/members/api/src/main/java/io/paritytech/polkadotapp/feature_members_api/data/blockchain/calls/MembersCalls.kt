package io.paritytech.polkadotapp.feature_members_api.data.blockchain.calls

import io.novasama.substrate_sdk_android.runtime.extrinsic.builder.ExtrinsicBuilder
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchPublicKey
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.autoEncodedArgs
import io.paritytech.polkadotapp.chains.util.Modules
import io.paritytech.polkadotapp.chains.util.call
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId

@JvmInline
value class MembersCalls(val extrinsicBuilder: ExtrinsicBuilder)

val ExtrinsicBuilder.members: MembersCalls
    get() = MembersCalls(this)

fun MembersCalls.selfInclude(
    identifier: RingCollectionId,
    member: BandersnatchPublicKey,
    callValidAt: Timestamp,
) {
    extrinsicBuilder.call(
        moduleName = Modules.MEMBERS,
        callName = "self_include",
        arguments = autoEncodedArgs(
            "identifier" to identifier,
            "member" to member,
            "call_valid_at" to callValidAt.toBigInteger(),
        ),
    )
}
