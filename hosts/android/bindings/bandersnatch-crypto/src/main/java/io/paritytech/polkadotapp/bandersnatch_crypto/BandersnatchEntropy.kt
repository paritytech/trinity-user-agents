package io.paritytech.polkadotapp.bandersnatch_crypto

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.annotations.TransientStruct
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.ByteArraySerializable
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import kotlinx.serialization.Serializable

@JvmInline
value class BandersnatchEntropy(val value: ByteArray)

@JvmInline
value class BandersnatchContext(val value: ByteArray) {
    val stringValue: String get() = value.decodeToString()

    companion object {
        fun fromString(stringValue: String): BandersnatchContext {
            return BandersnatchContext(stringValue.encodeToByteArray())
        }
    }
}

fun ByteArray.intoBandersnatchContext(): BandersnatchContext {
    return BandersnatchContext(this)
}

@Serializable
@TransientStruct
class BandersnatchAlias(val value: ByteArraySerializable) {
    override fun equals(other: Any?): Boolean {
        return this === other || other is BandersnatchAlias && this.value contentEquals other.value
    }

    override fun hashCode(): Int {
        return value.contentHashCode()
    }
}

typealias BandersnatchPublicKey = DataByteArray
typealias BandersnatchSignature = DataByteArray
typealias BandersnatchRingMembers = List<BandersnatchPublicKey>
typealias BandersnatchProof = DataByteArray

fun BandersnatchEntropy.memberKey(): BandersnatchPublicKey {
    return BandersnatchCrypto.derive_member_key(value).toDataByteArray()
}

fun BandersnatchEntropy.createProof(
    allMembers: List<BandersnatchPublicKey>,
    message: ByteArray,
    context: ByteArray,
    domainSize: BandersnatchDomainSize
): BandersnatchProofResult {
    require(memberKey() in allMembers) { "Prover member key must be in allMembers" }

    val result = BandersnatchCrypto.create_proof(value, allMembers.map { it.value }, context, message, domainSize.ordinal)
    return BandersnatchProofResult(
        proof = result.proof.toDataByteArray(),
        alias = BandersnatchAlias(result.alias),
    )
}

data class BandersnatchProofResult(val proof: BandersnatchProof, val alias: BandersnatchAlias)

fun BandersnatchEntropy.sign(
    message: ByteArray
): ByteArray {
    return BandersnatchCrypto.sign(value, message)
}

fun BandersnatchEntropy.aliasInContext(context: BandersnatchContext): BandersnatchAlias {
    return BandersnatchAlias(BandersnatchCrypto.alias_in_context(value, context.value))
}

data class ContextualAlias(val context: BandersnatchContext, val alias: BandersnatchAlias)
