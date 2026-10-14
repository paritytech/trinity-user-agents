package io.paritytech.polkadotapp.feature_statement_store_api.data

import io.paritytech.polkadotapp.feature_statement_store_api.data.models.StatementStoreMessageProof
import kotlinx.coroutines.flow.Flow

typealias StatementData = ByteArray
typealias StatementTopic = ByteArray
typealias StatementChannel = ByteArray

class Statement(
    val proof: StatementStoreMessageProof,
    val body: Body
) {
    class Body(
        val channel: StatementChannel? = null,
        val expiry: ULong,
        val topic1: StatementTopic? = null,
        val topic2: StatementTopic? = null,
        val topic3: StatementTopic? = null,
        val topic4: StatementTopic? = null,
        val data: StatementData
    )
}

interface StatementStoreService {
    suspend fun submitStatement(statement: Statement): Result<Unit>

    /**
     * Single-attempt variant of [submitStatement] — does not retry on RETRIABLE_FAILURE.
     */
    suspend fun submitStatementOnce(statement: Statement): Result<Unit>

    suspend fun fetchStatements(filter: TopicFilter): Result<List<Statement>>

    fun subscribeStatements(filter: TopicFilter): Flow<Result<StatementsPage>>
}
