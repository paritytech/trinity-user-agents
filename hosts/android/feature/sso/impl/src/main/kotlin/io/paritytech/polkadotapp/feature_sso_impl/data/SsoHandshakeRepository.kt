package io.paritytech.polkadotapp.feature_sso_impl.data

import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.prepareSignedStatement
import javax.inject.Inject

interface SsoHandshakeRepository {
    suspend fun submitHandshakeAnswer(
        body: Statement.Body,
        submitFrom: MetaAccount,
    ): Result<Unit>
}

class RealSsoHandshakeRepository @Inject constructor(
    private val statementStoreService: StatementStoreService,
    private val proverFactory: StatementStoreMessageProver.Factory,
) : SsoHandshakeRepository {
    override suspend fun submitHandshakeAnswer(
        body: Statement.Body,
        submitFrom: MetaAccount,
    ): Result<Unit> {
        val prover = proverFactory.createKeyPairProver(submitFrom)
        val statement = prover.prepareSignedStatement(body)

        return statementStoreService.submitStatement(statement)
    }
}
