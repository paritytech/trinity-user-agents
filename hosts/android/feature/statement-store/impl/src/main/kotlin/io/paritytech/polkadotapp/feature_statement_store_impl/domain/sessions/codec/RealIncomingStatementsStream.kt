package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map

/**
 * Subscribes to all topics from [topicsProvider] in a single multi-topic WebSocket call and
 * decodes each incoming statement via [StatementDecoder]. When the topic set changes
 * (peer adds/removes a device) the subscription is re-opened via [flatMapLatest].
 *
 * Each statement's [Statement.Body.topic1] is matched against [IncomingTopicSpec.topic] to
 * pick the correct sender encryption key for envelope unwrap.
 */
class RealIncomingStatementsStream(
    private val topicsProvider: IncomingTopicsProvider,
    private val statementStoreService: StatementStoreService,
    private val prover: StatementStoreMessageProver,
    private val decoder: StatementDecoder,
) : IncomingStatementsStream {
    override suspend fun fetch(): Result<List<StatementTransportEvent>> {
        val specs = topicsProvider.topics().first()
        val filter = TopicFilter.MatchAny(specs.map { it.topic })
        return statementStoreService.fetchStatements(filter)
            .map { statements -> statements.mapToEvents(specs) }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    override fun subscribe(): Flow<Result<List<StatementTransportEvent>>> {
        return topicsProvider.topics().flatMapLatest { specs ->
            val filter = TopicFilter.MatchAny(specs.map { it.topic })
            statementStoreService.subscribeStatements(filter).map { statementsResult ->
                statementsResult.map { it.statements.mapToEvents(specs) }
            }
        }
    }

    private suspend fun List<Statement>.mapToEvents(
        specs: List<IncomingTopicSpec>,
    ): List<StatementTransportEvent> {
        return mapNotNull { statement ->
            val isVerified = prover.verifyMessageProof(statement.body, statement.proof)
            if (isVerified.not()) return@mapNotNull null

            val spec = specs.findSpec(statement.body.topic1) ?: return@mapNotNull null

            val decoded = decoder.decode(statement, spec.senderEncryptionPublicKey, spec.encryption)
            decoded
        }
    }

    private fun List<IncomingTopicSpec>.findSpec(
        statementTopic: ByteArray?,
    ): IncomingTopicSpec? {
        if (statementTopic == null) return null
        return firstOrNull { it.topic.contentEquals(statementTopic) }
    }
}
