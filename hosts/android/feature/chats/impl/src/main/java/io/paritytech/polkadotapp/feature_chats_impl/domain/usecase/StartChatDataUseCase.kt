package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.x25519OrNull
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chain_resources_api.domain.model.ConsumerInfo
import io.paritytech.polkadotapp.feature_chats_api.domain.error.StartChatError
import io.paritytech.polkadotapp.feature_chats_api.domain.error.asStartChatError
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactOrigins
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.StartChatData
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import javax.inject.Inject

class StartChatDataUseCase @Inject constructor(
    knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val resourcesRepository: ResourcesRepository,
    private val accountRepository: AccountRepository,
    private val contactsRepository: ContactsRepository
) {
    private val chainId = knownChains.assetHub

    suspend operator fun invoke(contactAccountId: AccountId): Result<StartChatData> {
        val existingContact = contactsRepository.getContact(contactAccountId)
        if (existingContact != null) {
            return Result.success(StartChatData.ExistingChat(contactAccountId))
        }

        val chain = chainRegistry.getChain(chainId)

        return resourcesRepository.consumerInfo(chain.id, contactAccountId)
            .mapError(Throwable::asStartChatError)
            .flatMap { consumerInfo ->
                if (consumerInfo == null) {
                    Result.failure(StartChatError.PeerNotRegistered)
                } else {
                    runCancellableCatching { newChat(contactAccountId, consumerInfo) }
                        .mapError(Throwable::asStartChatError)
                }
            }
    }

    private suspend fun newChat(contactAccountId: AccountId, consumerInfo: ConsumerInfo): StartChatData {
        val walletAccount = accountRepository.getWalletAccount()

        return StartChatData.NewChat(
            contactAccountId = contactAccountId,
            username = Username.fromFullValue(consumerInfo.username),
            avatar = null,
            chatKey = consumerInfo.identifierKey.x25519OrNull() ?: error("Peer account uses an unsupported chat encryption key type"),
            sharedSecretDerivationDomain = SharedSecretDerivationDomain.CHAT,
            ourMetaAccountId = walletAccount.id,
            origin = ContactOrigins.CONTACT_CHAT
        )
    }
}
