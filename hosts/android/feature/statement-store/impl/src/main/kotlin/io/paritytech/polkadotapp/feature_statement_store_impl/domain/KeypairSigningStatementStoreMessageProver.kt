package io.paritytech.polkadotapp.feature_statement_store_impl.domain

import io.novasama.substrate_sdk_android.encrypt.SignatureVerifier
import io.novasama.substrate_sdk_android.encrypt.Signer
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.paritytech.polkadotapp.chains.util.signIntoWrapper
import io.paritytech.polkadotapp.chains.util.signing.MessageSigningContext
import io.paritytech.polkadotapp.common.utils.buildByteArray
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementStoreMessageProof
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.StatementFieldRemote
import io.paritytech.polkadotapp.feature_statement_store_impl.data.models.scale.toRemoteFields
import kotlinx.serialization.encodeToByteArray

class KeypairSigningStatementStoreMessageProver(
    private val getProvingKeypair: suspend () -> Sr25519Keypair,
) : StatementStoreMessageProver {
    override suspend fun generateMessageProof(statementBody: Statement.Body): StatementStoreMessageProof {
        val keypair = getProvingKeypair()
        val message = createProofPayload(statementBody)

        return StatementStoreMessageProof(
            signature = keypair.signIntoWrapper(message, MessageSigningContext.trustedContent()),
            publicKey = keypair.publicKey
        )
    }

    override suspend fun verifyMessageProof(
        statementBody: Statement.Body,
        proof: StatementStoreMessageProof
    ): Boolean {
        val message = createProofPayload(statementBody)

        return SignatureVerifier.verify(
            signatureWrapper = proof.signature,
            messageHashing = Signer.MessageHashing.SUBSTRATE,
            data = message,
            publicKey = proof.publicKey
        )
    }

    override suspend fun verifyBytes(data: ByteArray, proof: StatementStoreMessageProof): Boolean {
        return SignatureVerifier.verify(
            signatureWrapper = proof.signature,
            messageHashing = Signer.MessageHashing.SUBSTRATE,
            data = data,
            publicKey = proof.publicKey
        )
    }

    private fun createProofPayload(statementBody: Statement.Body): ByteArray {
        return buildByteArray {
            statementBody.toRemoteFields()
                .sortedBy { it.proofPayloadOrder }
                .map(BinaryScale::encodeToByteArray)
                .onEach { write(it) }
        }
    }

    private val StatementFieldRemote.proofPayloadOrder: Int
        get() = when (this) {
            is StatementFieldRemote.Expiry -> 0
            is StatementFieldRemote.Channel -> 1
            is StatementFieldRemote.Topic1 -> 2
            is StatementFieldRemote.Topic2 -> 3
            is StatementFieldRemote.Topic3 -> 4
            is StatementFieldRemote.Topic4 -> 5
            is StatementFieldRemote.Data -> 6
            is StatementFieldRemote.Proof -> error("Proof cannot be present in proof payload")
        }
}
