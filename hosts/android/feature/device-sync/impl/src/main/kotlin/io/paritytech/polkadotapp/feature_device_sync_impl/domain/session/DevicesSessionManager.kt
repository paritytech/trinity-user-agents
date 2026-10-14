package io.paritytech.polkadotapp.feature_device_sync_impl.domain.session

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.getChain
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CollectionDiffer
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.Identifiable
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.kilobytes
import io.paritytech.polkadotapp.common.utils.childScope
import io.paritytech.polkadotapp.common.utils.diffed
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.model.ActiveSsoSession
import io.paritytech.polkadotapp.feature_sso_api.domain.model.DeviceStatus
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSessionCreator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.OurDeviceKeypairProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Keeps an [OwnDeviceSession] alive for each active SSO-paired device. ECDH uses our
 * per-device P-256 ([OurDeviceKeypairProvider]) on our side and the peer's
 * [ActiveSsoSession.encryptionPublicKey] on the remote side.
 */
@Singleton
class DevicesSessionManager @Inject constructor(
    private val sessionCreatorFactory: CommunicationSessionCreator.Factory,
    private val communicationEncryptionFactory: CommunicationEncryption.Factory,
    private val ourDeviceKeypairProvider: OurDeviceKeypairProvider,
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val getActiveSsoSessionsUseCase: GetActiveSsoSessionsUseCase,
    dispatchers: CoroutineDispatchers,
) : CoroutineScope {
    private companion object {
        private val MAX_STATEMENT_SIZE = 2.kilobytes
    }

    override val coroutineContext = dispatchers.io + SupervisorJob()

    private val sessionsFlow = MutableStateFlow<Map<AccountId, OwnDeviceSession>>(emptyMap())

    private var creator: CommunicationSessionCreator? = null
    private var localSessionAccount: SessionAccount.Local? = null

    fun startSubscriptions() {
        getActiveSsoSessionsUseCase.observeSessions()
            .map { sessions -> sessions.filter { it.status == DeviceStatus.ACTIVE }.map(::DiffablePeer) }
            .diffed()
            .onEach(::applyDiff)
            .launchIn(this)
    }

    fun observeSessions(): Flow<List<OwnDeviceSession>> =
        sessionsFlow.map { it.values.toList() }

    private suspend fun applyDiff(diff: CollectionDiffer.Diff<DiffablePeer>) {
        val added = buildList {
            diff.added.forEach { peer ->
                createSession(peer.session)
                    .onSuccess { add(peer.session.statementAccountId to it) }
            }
        }
        val removedIds = diff.removed.map { it.session.statementAccountId }
        val replacedIds = added.map { it.first } + removedIds

        replacedIds.forEach { sessionsFlow.value[it]?.dispose() }

        sessionsFlow.update { current -> (current - replacedIds.toSet()) + added.toMap() }
    }

    private suspend fun createSession(peer: ActiveSsoSession) = runCatching {
        val sessionCreator = ensureCreator()
        val localAccount = requireNotNull(localSessionAccount) { "localSessionAccount initialised in ensureCreator()" }

        val encryption = communicationEncryptionFactory.createEncryption(
            localKeypair = ourDeviceKeypairProvider.get(),
            peerPublicKey = peer.encryptionPublicKey,
        )

        val remoteAccount = SessionAccount.Remote(
            accountId = peer.statementAccountId,
            pin = null,
            publicKey = peer.encryptionPublicKey,
        )

        val scope = childScope(supervised = true)
        val communicationSession = sessionCreator.createSession(
            scope = scope,
            localAccount = localAccount,
            remoteAccount = remoteAccount,
            encryption = encryption,
            maxStatementSize = MAX_STATEMENT_SIZE,
        )

        OwnDeviceSession(peer = peer, communicationSession = communicationSession, scope = scope)
    }.logFailure("DevicesSessionManager: failed to create session for device ${peer.statementAccountId}")

    private suspend fun ensureCreator(): CommunicationSessionCreator {
        creator?.let { return it }

        val walletAccount = accountRepository.getWalletAccount()
        val localAccountId = walletAccount.accountIdIn(chainRegistry.getChain(knownChains.people))
        localSessionAccount = SessionAccount.Local(accountId = localAccountId, pin = null)
        return sessionCreatorFactory.create(account = walletAccount).also { creator = it }
    }

    private class DiffablePeer(val session: ActiveSsoSession) : Identifiable {
        override val identifier: String = session.id
    }
}
