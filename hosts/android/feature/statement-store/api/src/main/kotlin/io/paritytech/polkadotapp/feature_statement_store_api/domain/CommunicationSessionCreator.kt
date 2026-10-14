package io.paritytech.polkadotapp.feature_statement_store_api.domain

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import kotlinx.coroutines.CoroutineScope

/**
 * Creates [CommunicationSession]s for pairwise messaging (SSO, video-game signalling,
 * chat). Supports both single-device and multi-device wire formats via separate
 * factory methods — callers work against the same [CommunicationSession] type regardless.
 *
 * Transitions between single- and multi-device modes are handled by destroying the old
 * session and creating a new one via the appropriate method.
 */
interface CommunicationSessionCreator {
    interface Factory {
        fun create(account: MetaAccount): CommunicationSessionCreator
    }

    /**
     * Subscribes to one identity topic and sends plain [Statement]s on it.
     * Incoming side also accepts multi-device envelopes (unwrapped with the identity
     * keypair).
     */
    fun createSession(
        scope: CoroutineScope,
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
        maxStatementSize: InformationSize,
        compactor: MessageCompactor? = null,
    ): CommunicationSession

    /**
     * Subscribes to the identity topic + all per-device topics of the remote peer and
     * sends per-device envelope-wrapped [Statement]s. Incoming side also accepts plain
     * single-device messages.
     */
    suspend fun createMultiDeviceSession(
        scope: CoroutineScope,
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        perDeviceEncryption: CommunicationEncryption,
        identityChatDomain: SharedSecretDerivationDomain,
        maxStatementSize: InformationSize,
        compactor: MessageCompactor? = null,
    ): CommunicationSession
}
