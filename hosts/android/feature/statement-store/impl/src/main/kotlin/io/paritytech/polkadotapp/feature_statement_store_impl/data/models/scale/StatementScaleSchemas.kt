package io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale

import io.novasama.substrate_sdk_android.encrypt.SignatureWrapper
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.EnumIndex
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.annotations.FixedLength
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementProofRemote
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementStoreMessageProof
import kotlinx.serialization.Serializable

@Serializable
sealed class StatementFieldRemote {
    @Serializable
    @EnumIndex(0)
    class Proof(val proof: StatementProofRemote) : StatementFieldRemote()

    @Serializable
    @EnumIndex(2)
    class Expiry(val expiry: ULong) : StatementFieldRemote()

    @Serializable
    @EnumIndex(3)
    class Channel(@FixedLength(32) val channel: ByteArray) : StatementFieldRemote()

    @Serializable
    @EnumIndex(4)
    class Topic1(@FixedLength(32) val topic: ByteArray) : StatementFieldRemote()

    @Serializable
    @EnumIndex(5)
    class Topic2(@FixedLength(32) val topic: ByteArray) : StatementFieldRemote()

    @Serializable
    @EnumIndex(6)
    class Topic3(@FixedLength(32) val topic: ByteArray) : StatementFieldRemote()

    @Serializable
    @EnumIndex(7)
    class Topic4(@FixedLength(32) val topic: ByteArray) : StatementFieldRemote()

    @Serializable
    @EnumIndex(8)
    class Data(val data: ByteArray) : StatementFieldRemote()
}

@JvmInline
@Serializable
value class StatementRemote(val fields: List<StatementFieldRemote>)

fun Statement.toRemote(): StatementRemote {
    return StatementRemote(
        fields = listOf(StatementFieldRemote.Proof(proof.toRemote())) +
            body.toRemoteFields()
    )
}

fun Statement.Body.toRemoteFields(): List<StatementFieldRemote> {
    return listOfNotNull(
        StatementFieldRemote.Expiry(expiry),
        channel?.let(StatementFieldRemote::Channel),
        topic1?.let(StatementFieldRemote::Topic1),
        topic2?.let(StatementFieldRemote::Topic2),
        topic3?.let(StatementFieldRemote::Topic3),
        topic4?.let(StatementFieldRemote::Topic4),
        StatementFieldRemote.Data(data)
    )
}

fun StatementRemote.toDomain(): Statement {
    var proof: StatementStoreMessageProof? = null
    var expiry: ULong? = null
    var channel: ByteArray? = null
    var topic1: ByteArray? = null
    var topic2: ByteArray? = null
    var topic3: ByteArray? = null
    var topic4: ByteArray? = null
    var data: ByteArray? = null

    fields.forEach { field ->
        when (field) {
            is StatementFieldRemote.Channel -> channel = field.channel
            is StatementFieldRemote.Data -> data = field.data
            is StatementFieldRemote.Expiry -> expiry = field.expiry
            is StatementFieldRemote.Proof -> proof = field.proof.toDomain()
            is StatementFieldRemote.Topic1 -> topic1 = field.topic
            is StatementFieldRemote.Topic2 -> topic2 = field.topic
            is StatementFieldRemote.Topic3 -> topic3 = field.topic
            is StatementFieldRemote.Topic4 -> topic4 = field.topic
        }
    }

    return Statement(
        proof = requireNotNull(proof) { "Proof was not present" },
        body = Statement.Body(
            channel = channel,
            expiry = requireNotNull(expiry) { "Expiry was not present" },
            topic1 = topic1,
            topic2 = topic2,
            topic3 = topic3,
            topic4 = topic4,
            data = requireNotNull(data) { "Data was not present" },
        )
    )
}

private fun StatementStoreMessageProof.toRemote(): StatementProofRemote {
    return StatementProofRemote.Sr25519(signature.signature, publicKey)
}

private fun StatementProofRemote.toDomain(): StatementStoreMessageProof {
    return when (this) {
        is StatementProofRemote.Sr25519 -> StatementStoreMessageProof(
            signature = SignatureWrapper.Sr25519(signature),
            publicKey = signer
        )
    }
}
