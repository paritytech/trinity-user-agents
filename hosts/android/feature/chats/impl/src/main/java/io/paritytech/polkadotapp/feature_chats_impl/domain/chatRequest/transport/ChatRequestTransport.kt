package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.ChatRequestCrypto
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.ChatRequestProver
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestEncryptedRemote
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.ChatRequestTopicDerivation
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementData
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementTopic
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccountParams
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import io.paritytech.polkadotapp.feature_statement_store_api.domain.prepareSignedStatement
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject

interface ChatRequestTransport {
    suspend fun fetchChatRequests(
        topic: ChatRequestTopic,
        derivationDomain: SharedSecretDerivationDomain,
    ): Result<List<ChatRequestDecrypted>>

    fun subscribeChatRequests(
        topic: ChatRequestTopic,
        derivationDomain: SharedSecretDerivationDomain,
    ): Flow<Result<List<ChatRequestDecrypted>>>

    suspend fun submitChatRequest(
        topics: OutgoingChatRequestTopics,
        request: ChatRequestDecrypted,
        derivationDomain: SharedSecretDerivationDomain,
        statementSigner: MetaAccount,
    ): Result<Unit>
}

class RealChatRequestTransport @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val statementStoreService: StatementStoreService,
    private val chatRequestCrypto: ChatRequestCrypto,
    private val chatRequestProver: ChatRequestProver,
    private val encryptionFactory: CommunicationEncryption.Factory,
    private val statementProverFactory: StatementStoreMessageProver.Factory
) : ChatRequestTransport {
    override suspend fun fetchChatRequests(
        topic: ChatRequestTopic,
        derivationDomain: SharedSecretDerivationDomain,
    ): Result<List<ChatRequestDecrypted>> {
        return withContext(coroutineDispatchers.io) {
            val statementTopic = topic.toStatementTopic(derivationDomain)

            Timber.d("Fetching chat requests for topic ${statementTopic.toHexString()} ($topic)")

            statementStoreService.fetchStatements(TopicFilter.MatchAll(listOf(statementTopic))).map { statements ->
                decryptAndVerifyStatements(statements, derivationDomain, topic.acceptor)
            }
        }
    }

    override fun subscribeChatRequests(
        topic: ChatRequestTopic,
        derivationDomain: SharedSecretDerivationDomain,
    ): Flow<Result<List<ChatRequestDecrypted>>> {
        return flow {
            val statementTopic = topic.toStatementTopic(derivationDomain)

            Timber.d("Subscribing to chat requests for topic ${statementTopic.toHexString()} ($topic)")

            val statements = statementStoreService.subscribeStatements(TopicFilter.MatchAll(listOf(statementTopic)))
                .map { pageResult ->
                    pageResult.map { page ->
                        decryptAndVerifyStatements(page.statements, derivationDomain, topic.acceptor)
                    }
                }

            emitAll(statements)
        }.flowOn(coroutineDispatchers.io)
    }

    override suspend fun submitChatRequest(
        topics: OutgoingChatRequestTopics,
        request: ChatRequestDecrypted,
        derivationDomain: SharedSecretDerivationDomain,
        statementSigner: MetaAccount,
    ): Result<Unit> {
        return encryptAndEncodeStatementData(request, peerPublicKey = topics.session.peerChatKey)
            .mapCatching { createChatRequestStatementBody(it, topics, derivationDomain) }
            .mapCatching { createChatRequestStatement(it, statementSigner) }
            .flatMap { statementStoreService.submitStatement(it) }
    }

    private suspend fun createChatRequestStatementBody(
        statementData: StatementData,
        topics: OutgoingChatRequestTopics,
        derivationDomain: SharedSecretDerivationDomain,
    ): Statement.Body {
        val statementBody = Statement.Body(
            expiry = StatementExpiry.createForCurrentTimestamp(),
            topic1 = topics.day.toStatementTopic(derivationDomain),
            topic2 = topics.full.toStatementTopic(derivationDomain),
            topic3 = topics.session.toStatementTopic(derivationDomain),
            data = statementData
        )
        return statementBody
    }

    private suspend fun createChatRequestStatement(
        statementBody: Statement.Body,
        statementSigner: MetaAccount,
    ): Statement {
        return statementProverFactory.createKeyPairProver(statementSigner)
            .prepareSignedStatement(statementBody)
    }

    private suspend fun ChatRequestTopic.toStatementTopic(
        derivationDomain: SharedSecretDerivationDomain,
    ): StatementTopic {
        return when (this) {
            is ChatRequestTopic.Day -> ChatRequestTopicDerivation.deriveDayTopic(acceptor, day)

            is ChatRequestTopic.Full -> ChatRequestTopicDerivation.deriveFullTopic(acceptor)

            is ChatRequestTopic.Session -> deriveSessionTopic(this, derivationDomain)
        }
    }

    private suspend fun deriveSessionTopic(
        topic: ChatRequestTopic.Session,
        derivationDomain: SharedSecretDerivationDomain,
    ): StatementTopic {
        val encryption = encryptionFactory.create(derivationDomain, peerPublicKey = topic.peerChatKey)

        return ChatRequestTopicDerivation.deriveSessionTopic(
            sharedSecret = encryption.sharedSecret.bytes.value,
            requester = SessionAccountParams(
                accountId = topic.requester,
                pin = topic.pin,
            ),
            acceptor = SessionAccountParams(
                accountId = topic.acceptor,
                pin = topic.pin
            ),
        )
    }

    private suspend fun decryptAndVerifyStatement(
        statement: Statement,
        decryptionDerivationDomain: SharedSecretDerivationDomain,
        acceptor: AccountId
    ): Result<ChatRequestDecrypted> {
        return decodeEncryptedRequest(statement)
            .flatMap { encrypted -> chatRequestCrypto.decrypt(encrypted, decryptionDerivationDomain) }
            .flatMap { decrypted -> verifyProof(decrypted, acceptor) }
    }

    private suspend fun decryptAndVerifyStatements(
        statements: List<Statement>,
        derivationDomain: SharedSecretDerivationDomain,
        acceptor: AccountId,
    ): List<ChatRequestDecrypted> {
        return statements.mapNotNull { statement ->
            decryptAndVerifyStatement(statement, derivationDomain, acceptor)
                .logFailure("Failed to decrypt and verify statement: $statement")
                .getOrNull()
        }
    }

    private suspend fun encryptAndEncodeStatementData(
        request: ChatRequestDecrypted,
        peerPublicKey: X25519PublicKey,
    ): Result<StatementData> {
        return chatRequestCrypto.encrypt(request, peerPublicKey)
            .mapCatching { BinaryScale.encodeToByteArray(it) }
    }

    private suspend fun verifyProof(
        decrypted: ChatRequestDecrypted,
        acceptor: AccountId,
    ): Result<ChatRequestDecrypted> {
        val verified = chatRequestProver.verifyProof(decrypted.message, acceptor, decrypted.proof)
            .logFailure("Failed to verify signature")
            .getOrDefault(false)

        return if (verified) {
            Result.success(decrypted)
        } else {
            Result.failure(IllegalStateException("Invalid chat request proof"))
        }
    }

    private fun decodeEncryptedRequest(statement: Statement): Result<ChatRequestEncryptedRemote> = runCatching {
        BinaryScale.decodeFromByteArray<ChatRequestEncryptedRemote>(statement.body.data)
    }
}
