package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OneShotSessionRequestPublisher
import io.paritytech.polkadotapp.feature_statement_store_api.domain.RequestId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.EncodedMessage
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import io.paritytech.polkadotapp.feature_statement_store_api.domain.prepareSignedStatement
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.models.StatementTransportEvent
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.SingleRequestOutgoingBodyBuilder
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.StatementChannelCreator
import javax.inject.Inject

class RealOneShotSessionRequestPublisher @Inject constructor(
    private val statementStoreService: StatementStoreService,
    private val channelCreator: StatementChannelCreator,
) : OneShotSessionRequestPublisher {
    override suspend fun publish(
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
        requestId: RequestId,
        messages: List<EncodedMessage>,
        prover: StatementStoreMessageProver,
    ): Result<Unit> = runCancellableCatching {
        val outgoingTopic = deriveCommunicationTopic(localAccount, remoteAccount, encryption)
        val request = StatementTransportEvent.Request(
            requestId = requestId,
            expiry = StatementExpiry.createForCurrentTimestamp(),
            messages = messages,
        )
        val body = SingleRequestOutgoingBodyBuilder(outgoingTopic, encryption, channelCreator).buildRequestBody(request)

        prover.prepareSignedStatement(body)
    }.flatMap { statement -> statementStoreService.submitStatement(statement) }

    override suspend fun isStoredBy(
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
        signer: AccountId,
    ): Result<Boolean> {
        val outgoingTopic = deriveCommunicationTopic(localAccount, remoteAccount, encryption)
        val requestChannel = channelCreator.requestChannel(outgoingTopic)

        return statementStoreService.fetchStatements(TopicFilter.MatchAll(listOf(outgoingTopic))).map { statements ->
            statements.any { statement ->
                statement.proof.publicKey.contentEquals(signer.value) && statement.body.channel?.contentEquals(requestChannel) == true
            }
        }
    }
}
