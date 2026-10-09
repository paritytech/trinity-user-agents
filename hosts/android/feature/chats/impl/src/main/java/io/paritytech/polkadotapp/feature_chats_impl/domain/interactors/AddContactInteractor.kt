package io.paritytech.polkadotapp.feature_chats_impl.domain.interactors

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.mapList
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getWalletAccountIdIn
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isContactChat
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatSearchRecentsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ContactSearchResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.StartChatData
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.StartChatDataUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SubscribeActiveChatsUseCase
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.SearchUsernamesUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

interface AddContactInteractor {
    suspend fun searchContacts(query: String): Result<List<ContactSearchResult>>
    suspend fun getStartChatData(contactAccountId: AccountId): Result<StartChatData>

    context(scope: ComputationalScope)
    fun observeRecentChats(): Flow<List<Chat>>

    fun observeBlockedAccountIds(): Flow<Set<AccountId>>

    suspend fun addRecent(chatId: ChatId)
}

class RealAddContactInteractor @Inject constructor(
    private val knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val accountRepository: AccountRepository,
    private val searchUsernamesUseCase: SearchUsernamesUseCase,
    private val startChatDataUseCase: StartChatDataUseCase,
    private val subscribeActiveChats: SubscribeActiveChatsUseCase,
    private val chatSearchRecentsRepository: ChatSearchRecentsRepository,
    private val contactsRepository: ContactsRepository,
) : AddContactInteractor {
    override suspend fun searchContacts(query: String): Result<List<ContactSearchResult>> {
        val ownAccountId = accountRepository.getWalletAccountIdIn(chainRegistry.getChain(knownChains.people))
        return searchUsernamesUseCase(query)
            .map { list ->
                list.distinctBy { it.accountId }
                    .filterNot { it.accountId == ownAccountId }
            }
            .mapList {
                ContactSearchResult(
                    accountId = it.accountId,
                    username = it.username
                )
            }
    }

    override suspend fun getStartChatData(contactAccountId: AccountId): Result<StartChatData> {
        return startChatDataUseCase(contactAccountId)
    }

    context(scope: ComputationalScope)
    override fun observeRecentChats(): Flow<List<Chat>> = subscribeActiveChats()
        .map { chats -> chats.filter { it.id.isContactChat() } }

    override fun observeBlockedAccountIds(): Flow<Set<AccountId>> = contactsRepository.subscribeBlockedContacts()
        .map { contacts -> contacts.map { it.accountId }.toSet() }

    override suspend fun addRecent(chatId: ChatId) {
        chatSearchRecentsRepository.addRecent(chatId)
    }
}
