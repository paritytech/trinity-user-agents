package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.prepareSignedStatement
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.IncomingStatementsStream
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.OutgoingBodyBuilder
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.StatementDecoder
import kotlinx.coroutines.flow.Flow

/**
 * Wire-level transport for [StatementTransportEvent]s — one instance per
 * [io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession]. Topic and
 * encryption strategy are fixed at construction; the transport never branches on wire format
 * at runtime.
 */
interface CommunicationTransport {
    suspend fun fetchOutgoing(): Result<List<StatementTransportEvent>>

    suspend fun fetchIncoming(): Result<List<StatementTransportEvent>>

    fun subscribeIncoming(): Flow<Result<List<StatementTransportEvent>>>

    suspend fun submitRequest(request: StatementTransportEvent.Request): Result<Unit>

    suspend fun submitResponse(response: StatementTransportEvent.Response): Result<Unit>
}

class RealCommunicationTransport(
    private val outgoingTopic: CommunicationSessionId,
    private val outgoingBuilder: OutgoingBodyBuilder,
    private val incomingStream: IncomingStatementsStream,
    private val decoder: StatementDecoder,
    private val statementStoreService: StatementStoreService,
    private val prover: StatementStoreMessageProver,
) : CommunicationTransport {
    override suspend fun fetchOutgoing(): Result<List<StatementTransportEvent>> {
        return statementStoreService.fetchStatements(TopicFilter.MatchAll(listOf(outgoingTopic))).map { statements ->
            statements.mapNotNull { statement ->
                val isVerified = prover.verifyMessageProof(statement.body, statement.proof)
                if (isVerified.not()) return@mapNotNull null

                decoder.decodeOur(statement)
            }
        }
    }

    override suspend fun fetchIncoming(): Result<List<StatementTransportEvent>> = incomingStream.fetch()

    override fun subscribeIncoming(): Flow<Result<List<StatementTransportEvent>>> = incomingStream.subscribe()

    override suspend fun submitRequest(request: StatementTransportEvent.Request): Result<Unit> =
        submit(outgoingBuilder.buildRequestBody(request))

    override suspend fun submitResponse(response: StatementTransportEvent.Response): Result<Unit> =
        submit(outgoingBuilder.buildResponseBody(response))

    private suspend fun submit(body: Statement.Body): Result<Unit> =
        runCatching { prover.prepareSignedStatement(body) }
            .flatMap { statementStoreService.submitStatement(it) }
}
