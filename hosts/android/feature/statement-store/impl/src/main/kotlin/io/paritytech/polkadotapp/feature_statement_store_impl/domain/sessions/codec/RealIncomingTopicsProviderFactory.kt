package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.ContactDeviceProvider
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.deriveCommunicationTopic
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class RealIncomingTopicsProviderFactory @Inject constructor(
    private val contactDeviceProvider: ContactDeviceProvider,
    private val encryptionFactory: CommunicationEncryption.Factory
) : IncomingTopicsProvider.Factory {
    override fun multiDeviceTopics(
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        identityChatDomain: SharedSecretDerivationDomain,
    ): IncomingTopicsProvider = IncomingTopicsProvider {
        contactDeviceProvider.observeDevices(remoteAccount.accountId).map { contactDevices ->
            contactDevices.map { device ->
                val deviceRemote = SessionAccount.Remote(
                    accountId = device.statementAccountId,
                    pin = remoteAccount.pin,
                    publicKey = device.encryptionPublicKey,
                )
                val perPeerDeviceEncryption = encryptionFactory.create(
                    domain = identityChatDomain,
                    peerPublicKey = device.encryptionPublicKey,
                )
                val topic = deriveCommunicationTopic(deviceRemote, localAccount, perPeerDeviceEncryption)
                IncomingTopicSpec(
                    topic = topic,
                    senderEncryptionPublicKey = device.encryptionPublicKey,
                    encryption = perPeerDeviceEncryption,
                )
            }
        }
    }

    override fun identityTopicOnly(
        localAccount: SessionAccount.Local,
        remoteAccount: SessionAccount.Remote,
        encryption: CommunicationEncryption,
    ): IncomingTopicsProvider {
        val spec = IncomingTopicSpec(
            topic = deriveCommunicationTopic(remoteAccount, localAccount, encryption),
            senderEncryptionPublicKey = remoteAccount.publicKey,
            encryption = encryption,
        )
        return IncomingTopicsProvider { flowOf(listOf(spec)) }
    }
}
