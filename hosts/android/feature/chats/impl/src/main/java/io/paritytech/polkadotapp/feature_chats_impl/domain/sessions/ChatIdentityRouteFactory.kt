package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getAccountByIdOrThrow
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_statement_store_api.data.encryption.CommunicationEncryption
import io.paritytech.polkadotapp.feature_statement_store_api.domain.models.SessionAccount
import javax.inject.Inject

/** The identity route of a contact chat: `SessionId(us, contact)` keyed by the identity chat keys. */
class ChatIdentityRoute(
    val localAccount: SessionAccount.Local,
    val remoteAccount: SessionAccount.Remote,
    val encryption: CommunicationEncryption,
)

class ChatIdentityRouteFactory @Inject constructor(
    private val accountRepository: AccountRepository,
    private val chainRegistry: ChainRegistry,
    private val knownChains: KnownChains,
    private val encryptionFactory: CommunicationEncryption.Factory,
) {
    suspend fun create(contact: Contact): Result<ChatIdentityRoute> = runCancellableCatching {
        val ourAccount = accountRepository.getAccountByIdOrThrow(contact.ourMetaAccountId)
        val ourAccountId = ourAccount.accountIdIn(chainRegistry.getChain(knownChains.people))

        ChatIdentityRoute(
            localAccount = SessionAccount.Local(ourAccountId, null),
            remoteAccount = SessionAccount.Remote(accountId = contact.accountId, pin = contact.pin, publicKey = contact.chatKey),
            encryption = encryptionFactory.create(domain = contact.sharedSecretDerivationDomain, peerPublicKey = contact.chatKey),
        )
    }
}
