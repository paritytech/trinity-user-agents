package io.paritytech.polkadotapp.feature_sso_impl.domain.session

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_sso_impl.data.encryption.SsoDerivationDomains
import io.paritytech.polkadotapp.feature_sso_impl.data.repository.SsoSessionRepository
import io.paritytech.polkadotapp.feature_sso_impl.domain.model.SsoSessionData
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionId
import io.paritytech.polkadotapp.feature_sso_impl.domain.session.model.SsoSessionRequest
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSessionCreator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import timber.log.Timber
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SsoSessionManager @Inject constructor(
    private val communicationSessionCreatorFactory: CommunicationSessionCreator.Factory,
    private val ssoSessionRepository: SsoSessionRepository,
    private val accountRepository: AccountRepository,
    private val encryptionFactory: CommunicationEncryption.Factory,
    private val dotNsTldProvider: DotNsTldProvider,
    dispatchers: CoroutineDispatchers
) : CoroutineScope {
    companion object {
        private val MAX_STATEMENT_SIZE = 256.kilobytes
    }

    override val coroutineContext = dispatchers.io + SupervisorJob()

    private val sessionsFlow = MutableStateFlow<Map<SsoSessionId, SsoCommunicationSession>>(emptyMap())

    private var communicationSessionCreator: CommunicationSessionCreator? = null

    val allMessages: Flow<SsoSessionRequest>
        get() = sessionsFlow.flatMapLatest {
            it.values.map { it.requests }
                .merge()
        }.distinctUntilChanged()

    fun getSession(sessionId: SsoSessionId): SsoCommunicationSession? {
        return sessionsFlow.value[sessionId]
    }

    suspend fun allSessions(): Map<SsoSessionId, SsoCommunicationSession> {
        return sessionsFlow.first()
    }

    suspend fun awaitSession(sessionId: SsoSessionId): SsoCommunicationSession {
        return sessionsFlow
            .mapNotNull { it[sessionId] }
            .first()
    }

    suspend fun disconnectSession(sessionId: SsoSessionId) {
        val session = getSession(sessionId)

        if (session != null) {
            val disconnectRequest = SsoSessionRequest(
                sessionId = sessionId,
                requestId = UUID.randomUUID().toString(),
                content = SsoSessionRequest.Content.Disconnected
            )
            session.sendRequestAndAwaitSent(disconnectRequest)
                .logFailure("Failed to disconnect session")
        }

        deleteSession(sessionId)
    }

    suspend fun deleteSession(sessionId: SsoSessionId) {
        ssoSessionRepository.deleteSession(sessionId)
    }

    suspend fun init() {
        val account = getLocalAccountForSessions()
        communicationSessionCreator = communicationSessionCreatorFactory.create(account)

        subscribeToSessionsUpdates(account)
    }

    fun dispose() {
        cancel()
        sessionsFlow.update { sessions ->
            sessions.values.forEach { it.dispose() }
            emptyMap()
        }
        communicationSessionCreator = null
    }

    private fun subscribeToSessionsUpdates(account: MetaAccount) {
        val localAccountId = account.defaultAccountId()
        val localSessionAccount = SessionAccount.Local(localAccountId, null)

        ssoSessionRepository
            .observeSessions()
            .diffed()
            .onEach { diff ->
                diff.added.forEach { createSession(localSessionAccount, it) }
                diff.removed.forEach { removeSession(it) }
            }
            .launchIn(this)
    }

    private suspend fun createSession(
        localSessionAccount: SessionAccount.Local,
        sessionData: SsoSessionData
    ) {
        if (sessionsFlow.value.containsKey(sessionData.id)) {
            removeSession(sessionData)
        }

        val sessionCreator = communicationSessionCreator ?: return

        val scope = childScope(supervised = true)

        val encryption = encryptionFactory.create(
            SsoDerivationDomains.SSO_DERIVATION_DOMAIN,
            sessionData.sharedSecretPublicKey
        )

        val remoteAccount = SessionAccount.Remote(
            accountId = sessionData.statementStorePublicKey,
            pin = null,
            publicKey = sessionData.sharedSecretPublicKey
        )

        val communicationSession = sessionCreator.createSession(
            scope = scope,
            localAccount = localSessionAccount,
            remoteAccount = remoteAccount,
            encryption = encryption,
            maxStatementSize = MAX_STATEMENT_SIZE
        )

        val ssoCommunicationSession = SsoCommunicationSession(
            scope = scope,
            session = sessionData,
            communicationSession = communicationSession,
            dotNsTldProvider = dotNsTldProvider
        )

        sessionsFlow.update {
            it + (sessionData.id to ssoCommunicationSession)
        }

        Timber.d("SSO session created for ${sessionData.name}")
    }

    private fun removeSession(sessionData: SsoSessionData) {
        sessionsFlow.update {
            val session = it[sessionData.id]
            session?.dispose()
            it - sessionData.id
        }

        Timber.d("SSO session removed for ${sessionData.name}")
    }

    private suspend fun getLocalAccountForSessions(): MetaAccount {
        return accountRepository.getWalletAccount()
    }
}
