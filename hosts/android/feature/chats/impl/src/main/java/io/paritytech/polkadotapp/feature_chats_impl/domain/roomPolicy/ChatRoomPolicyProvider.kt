package io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.isExtensionChat
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import kotlinx.coroutines.flow.flowOf as flowOfValue

/** The one source of [ChatRoomPolicy] for the chat list, the room screen, the menus and the input. */
interface ChatRoomPolicyProvider {
    /** Emits null for a person chat. */
    fun observePolicy(chatId: ChatId): Flow<ChatRoomPolicy?>

    /** Contains a policy for each product room of [chatIds] and no entry for a person chat. */
    fun observePolicies(chatIds: Set<ChatId>): Flow<Map<ChatId, ChatRoomPolicy>>
}

class RealChatRoomPolicyProvider @Inject constructor(
    private val declarationsRepository: ChatRoomDeclarationsRepository,
) : ChatRoomPolicyProvider {
    override fun observePolicy(chatId: ChatId): Flow<ChatRoomPolicy?> {
        if (!chatId.isExtensionChat()) return flowOfValue(null)

        return declarationsRepository.observe(chatId)
            .map { ChatRoomPolicy.from(it) }
            .distinctUntilChanged()
    }

    override fun observePolicies(chatIds: Set<ChatId>): Flow<Map<ChatId, ChatRoomPolicy>> {
        val roomIds = chatIds.filter { it.isExtensionChat() }
        if (roomIds.isEmpty()) return flowOfValue(emptyMap())

        val policies = roomIds.map { chatId ->
            declarationsRepository.observe(chatId).map { chatId to ChatRoomPolicy.from(it) }
        }

        return combine(policies) { it.toMap() }.distinctUntilChanged()
    }
}
