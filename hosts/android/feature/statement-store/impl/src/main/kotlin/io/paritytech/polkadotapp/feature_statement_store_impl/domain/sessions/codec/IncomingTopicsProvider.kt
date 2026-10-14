package io.paritytech.polkadotapp.feature_statement_store_impl.domain.sessions.codec

import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import kotlinx.coroutines.flow.Flow

fun interface IncomingTopicsProvider {
    fun topics(): Flow<List<IncomingTopicSpec>>

    interface Factory {
        fun multiDeviceTopics(
            localAccount: SessionAccount.Local,
            remoteAccount: SessionAccount.Remote,
            identityChatDomain: SharedSecretDerivationDomain,
        ): IncomingTopicsProvider

        fun identityTopicOnly(
            localAccount: SessionAccount.Local,
            remoteAccount: SessionAccount.Remote,
            encryption: CommunicationEncryption,
        ): IncomingTopicsProvider
    }
}
