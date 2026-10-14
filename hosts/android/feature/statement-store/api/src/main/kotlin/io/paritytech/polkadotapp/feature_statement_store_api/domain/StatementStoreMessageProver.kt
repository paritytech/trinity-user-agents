package io.paritytech.polkadotapp.feature_statement_store_api.domain

import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementStoreMessageProof

interface StatementStoreMessageProver {
    interface Factory {
        fun createKeyPairProver(metaAccount: MetaAccount): StatementStoreMessageProver

        fun createKeyPairProver(keypair: Sr25519Keypair): StatementStoreMessageProver
    }

    suspend fun generateMessageProof(statementBody: Statement.Body): StatementStoreMessageProof

    suspend fun verifyMessageProof(
        statementBody: Statement.Body,
        proof: StatementStoreMessageProof
    ): Boolean

    /**
     * Verifies a signature over arbitrary bytes.
     */
    suspend fun verifyBytes(data: ByteArray, proof: StatementStoreMessageProof): Boolean
}

suspend fun StatementStoreMessageProver.prepareSignedStatement(statementBody: Statement.Body): Statement {
    val proof = generateMessageProof(statementBody)
    return Statement(proof, statementBody)
}
