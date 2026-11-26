package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport

import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.BinaryScale
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.decodeFromByteArray
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.binary.encodeToByteArray
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.exponentialRetryDelay
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.ChatRequestCrypto
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.ChatRequestProver
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestEncryptedRemote
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.ChatRequestTopicDerivation
import io.paritytech.polkadotapp.feature_statement_store_api.data.Statement
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementData
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementSubmitError
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementTopic
import io.paritytech.polkadotapp.feature_statement_store_api.data.TopicFilter
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccountParams
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.StatementExpiry
import io.paritytech.polkadotapp.feature_statement_store_api.domain.prepareSignedStatement
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

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
        statementProver: StatementStoreMessageProver,
    ): Result<Unit>

    suspend fun submitChatRequestCancel(
        topics: OutgoingChatRequestTopics,
        cancel: ChatRequestDecrypted,
        derivationDomain: SharedSecretDerivationDomain,
        statementProver: StatementStoreMessageProver,
        replacedCopyExpiry: ULong,
    ): Result<Unit>

    /** Upper bound of the encoded statement [submitChatRequest] would submit for [request]. */
    suspend fun estimateStatementSize(
        topics: OutgoingChatRequestTopics,
        request: ChatRequestDecrypted,
    ): Result<InformationSize>

    /** Whether a request signed by [signer] is still stored under [session]. */
    suspend fun isChatRequestStored(
        session: ChatRequestTopic.Session,
        derivationDomain: SharedSecretDerivationDomain,
        signer: AccountId,
    ): Result<Boolean>

    suspend fun fetchChatRequestCopiesToCancel(
        session: ChatRequestTopic.Session,
        derivationDomain: SharedSecretDerivationDomain,
    ): Result<List<StoredChatRequestCopy>>
}

class StoredChatRequestCopy(val signerAccountId: AccountId, val expiry: ULong)

// Generous upper bound on what a chat request statement adds around its data: proof, expiry, topics, length prefixes.
private val STATEMENT_ENVELOPE_SIZE = 512.bytes

private const val CANCEL_ATTEMPTS = 10
private val CANCEL_RETRY_DELAY = exponentialRetryDelay(initialDelay = 1.seconds, maxDelay = 256.seconds)

class RealChatRequestTransport @Inject constructor(
    private val coroutineDispatchers: CoroutineDispatchers,
    private val statementStoreService: StatementStoreService,
    private val chatRequestCrypto: ChatRequestCrypto,
    private val chatRequestProver: ChatRequestProver,
    private val encryptionFactory: CommunicationEncryption.Factory,
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
        statementProver: StatementStoreMessageProver,
    ): Result<Unit> {
        return prepareStatement(topics, request, derivationDomain, statementProver, StatementExpiry.createForCurrentTimestamp())
            .flatMap { statementStoreService.submitStatement(it) }
    }

    override suspend fun submitChatRequestCancel(
        topics: OutgoingChatRequestTopics,
        cancel: ChatRequestDecrypted,
        derivationDomain: SharedSecretDerivationDomain,
        statementProver: StatementStoreMessageProver,
        replacedCopyExpiry: ULong,
    ): Result<Unit> {
        return retryingCancelFailures {
            val expiry = StatementExpiry.nextAfter(replacedCopyExpiry)

            prepareStatement(topics, cancel, derivationDomain, statementProver, expiry)
                .flatMap { statementStoreService.submitStatementOnce(it) }
        }
    }

    private suspend fun prepareStatement(
        topics: OutgoingChatRequestTopics,
        request: ChatRequestDecrypted,
        derivationDomain: SharedSecretDerivationDomain,
        statementProver: StatementStoreMessageProver,
        expiry: ULong,
    ): Result<Statement> {
        return encryptAndEncodeStatementData(request, peerPublicKey = topics.session.peerChatKey)
            .mapCatching { createChatRequestStatementBody(it, topics, derivationDomain, expiry) }
            .mapCatching { statementProver.prepareSignedStatement(it) }
    }

    override suspend fun estimateStatementSize(
        topics: OutgoingChatRequestTopics,
        request: ChatRequestDecrypted,
    ): Result<InformationSize> {
        return encryptAndEncodeStatementData(request, peerPublicKey = topics.session.peerChatKey)
            .map { statementData -> statementData.size.bytes + STATEMENT_ENVELOPE_SIZE }
    }

    override suspend fun isChatRequestStored(
        session: ChatRequestTopic.Session,
        derivationDomain: SharedSecretDerivationDomain,
        signer: AccountId,
    ): Result<Boolean> {
        return fetchChatRequestCopies(session, derivationDomain).map { copies ->
            copies.any { it.signerAccountId == signer }
        }
    }

    override suspend fun fetchChatRequestCopiesToCancel(
        session: ChatRequestTopic.Session,
        derivationDomain: SharedSecretDerivationDomain,
    ): Result<List<StoredChatRequestCopy>> {
        return retryingCancelFailures { fetchChatRequestCopies(session, derivationDomain) }
    }

    private suspend fun fetchChatRequestCopies(
        session: ChatRequestTopic.Session,
        derivationDomain: SharedSecretDerivationDomain,
    ): Result<List<StoredChatRequestCopy>> = withContext(coroutineDispatchers.io) {
        val sessionTopic = session.toStatementTopic(derivationDomain)

        statementStoreService.fetchStatements(TopicFilter.MatchAll(listOf(sessionTopic))).map { statements ->
            statements.map { StoredChatRequestCopy(signerAccountId = it.proof.publicKey.intoAccountId(), expiry = it.body.expiry) }
        }
    }

    private suspend fun <T> retryingCancelFailures(action: suspend () -> Result<T>): Result<T> {
        var attempt = 1

        while (true) {
            val result = action()
            val error = result.exceptionOrNull()
            if (error == null || !error.allowsCancelRetry() || attempt == CANCEL_ATTEMPTS) return result

            delay(CANCEL_RETRY_DELAY.delayFor(attempt))
            attempt++
        }
    }

    private fun Throwable.allowsCancelRetry(): Boolean {
        return when (this) {
            is StatementSubmitError.ChannelPriorityTooLow -> true
            is StatementSubmitError -> false
            else -> true
        }
    }

    private suspend fun createChatRequestStatementBody(
        statementData: StatementData,
        topics: OutgoingChatRequestTopics,
        derivationDomain: SharedSecretDerivationDomain,
        expiry: ULong,
    ): Statement.Body {
        val sessionTopic = topics.session.toStatementTopic(derivationDomain)
        val statementBody = Statement.Body(
            channel = sessionTopic,
            expiry = expiry,
            topic1 = topics.day.toStatementTopic(derivationDomain),
            topic2 = topics.full.toStatementTopic(derivationDomain),
            topic3 = sessionTopic,
            data = statementData
        )
        return statementBody
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
