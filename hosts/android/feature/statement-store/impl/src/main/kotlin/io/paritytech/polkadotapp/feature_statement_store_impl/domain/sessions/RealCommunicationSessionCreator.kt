package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions

import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_statement_store_api.data.StatementStoreService
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSession
import io.paritytech.polkadotapp.feature_statement_store_api.domain.CommunicationSessionCreator
import io.paritytech.polkadotapp.feature_statement_store_api.domain.MessageCompactor
import io.paritytech.polkadotapp.feature_statement_store_api.domain.StatementStoreMessageProver
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.CommunicationSessionId
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.ContactDeviceProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.MultiDeviceEnvelopeEncryption
import io.paritytech.polkadotapp.feature_statement_store_impl.data.encryption.toEnvelopeRecipients
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.IncomingTopicsProvider
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.MultiDeviceOutgoingBodyBuilder
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.OutgoingBodyBuilder
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.RealIncomingStatementsStream
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.SingleRequestOutgoingBodyBuilder
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.StatementChannelCreator
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec.StatementDecoder
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

class CommunicationSessionCreatorFactory @Inject constructor(
    private val statementStoreService: StatementStoreService,
    private val proverFactory: StatementStoreMessageProver.Factory,
    private val channelCreator: StatementChannelCreator,
    private val envelopeEncryptionFactory: MultiDeviceEnvelopeEncryption.Factory,
    private val topicsProviderFactory: IncomingTopicsProvider.Factory,
    private val contactDeviceProvider: ContactDeviceProvider,
    private val encryptionFactory: CommunicationEncryption.Factory,
) : CommunicationSessionCreator.Factory {
    override fun create(account: MetaAccount): CommunicationSessionCreator {
        val prover = proverFactory.createKeyPairProver(account)
        return RealCommunicationSessionCreator(
            statementStoreService = statementStoreService,
            prover = prover,
            channelCreator = channelCreator,
            envelopeEncryptionFactory = envelopeEncryptionFactory,
            topicsProviderFactory = topicsProviderFactory,
            contactDeviceProvider = contactDeviceProvider,
            encryptionFactory = encryptionFactory,
        )
    }
}

class RealCommunicationSessionCreator(
    private val statementStoreService: StatementStoreService,
    private val prover: StatementStoreMessageProver,
    private val channelCreator: StatementChannelCreator,
    private val envelopeEncryptionFactory: MultiDeviceEnvelopeEncryption.Factory,
    private val topicsProviderFactory: IncomingTopicsProvider.Factory,
    private val contactDeviceProvider: ContactDeviceProvider,
    private val encryptionFactory: CommunicationEncryption.Factory,
) : CommunicationSessionCreator {
    override fun createSession(
        scope: CoroutineScope,
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
        maxStatementSize: InformationSize,
        compactor: MessageCompactor?,
    ): CommunicationSession {
        val outgoingTopic = deriveCommunicationTopic(localAccount, remoteAccount, encryption)
        val envelopeEncryption = buildEnvelopeEncryption(localAccount)

        val outgoingBuilder = SingleRequestOutgoingBodyBuilder(
            outgoingTopic = outgoingTopic,
            encryption = encryption,
            channelCreator = channelCreator,
        )

        val topicsProvider = topicsProviderFactory.identityTopicOnly(
            localAccount = localAccount,
            remoteAccount = remoteAccount,
            encryption = encryption,
        )

        return buildSession(
            scope = scope,
            localAccount = localAccount,
            remoteAccount = remoteAccount,
            encryption = encryption,
            envelopeEncryption = envelopeEncryption,
            outgoingTopic = outgoingTopic,
            outgoingBuilder = outgoingBuilder,
            topicsProvider = topicsProvider,
            peerDevices = { contactDeviceProvider.getDevices(remoteAccount.accountId).toEnvelopeRecipients() },
            maxStatementSize = maxStatementSize,
            compactor = compactor,
        )
    }

    override suspend fun createMultiDeviceSession(
        scope: CoroutineScope,
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        perDeviceEncryption: CommunicationEncryption,
        identityChatDomain: SharedSecretDerivationDomain,
        maxStatementSize: InformationSize,
        compactor: MessageCompactor?,
    ): CommunicationSession {
        val outgoingTopic = deriveCommunicationTopic(localAccount, remoteAccount, perDeviceEncryption)
        val envelopeEncryption = buildEnvelopeEncryption(localAccount)

        val outgoingBuilder = MultiDeviceOutgoingBodyBuilder(
            outgoingTopic = outgoingTopic,
            encryption = perDeviceEncryption,
            envelopeEncryption = envelopeEncryption,
            remoteAccountId = remoteAccount.accountId,
            contactDeviceProvider = contactDeviceProvider,
            channelCreator = channelCreator,
        )

        val topicsProvider = topicsProviderFactory.multiDeviceTopics(
            localAccount = localAccount,
            remoteAccount = remoteAccount,
            identityChatDomain = identityChatDomain,
        )

        return buildSession(
            scope = scope,
            localAccount = localAccount,
            remoteAccount = remoteAccount,
            encryption = perDeviceEncryption,
            envelopeEncryption = envelopeEncryption,
            outgoingTopic = outgoingTopic,
            outgoingBuilder = outgoingBuilder,
            topicsProvider = topicsProvider,
            peerDevices = { contactDeviceProvider.getDevices(remoteAccount.accountId).toEnvelopeRecipients() },
            maxStatementSize = maxStatementSize,
            compactor = compactor,
        )
    }

    private fun buildEnvelopeEncryption(
        localAccount: SessionAccount.Local
    ): MultiDeviceEnvelopeEncryption = envelopeEncryptionFactory.create(
        ourStatementAccountId = localAccount.accountId
    )

    private fun buildSession(
        scope: CoroutineScope,
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
        envelopeEncryption: MultiDeviceEnvelopeEncryption,
        outgoingTopic: CommunicationSessionId,
        outgoingBuilder: OutgoingBodyBuilder,
        topicsProvider: IncomingTopicsProvider,
        peerDevices: suspend () -> List<MultiDeviceEnvelopeEncryption.Recipient>,
        maxStatementSize: InformationSize,
        compactor: MessageCompactor?,
    ): CommunicationSession {
        val decoder = StatementDecoder(
            encryption = encryption,
            envelopeEncryption = envelopeEncryption,
            peerDevices = peerDevices,
        )

        val incomingStream = RealIncomingStatementsStream(
            topicsProvider = topicsProvider,
            statementStoreService = statementStoreService,
            prover = prover,
            decoder = decoder,
        )

        val transport = RealCommunicationTransport(
            outgoingTopic = outgoingTopic,
            outgoingBuilder = outgoingBuilder,
            incomingStream = incomingStream,
            decoder = decoder,
            statementStoreService = statementStoreService,
            prover = prover,
        )

        return RealCommunicationSession(
            localAccount = localAccount,
            remoteAccount = remoteAccount,
            communicationTransport = transport,
            encryption = encryption,
            maxStatementSize = maxStatementSize,
            compactor = compactor,
            scope = scope,
        )
    }
}
