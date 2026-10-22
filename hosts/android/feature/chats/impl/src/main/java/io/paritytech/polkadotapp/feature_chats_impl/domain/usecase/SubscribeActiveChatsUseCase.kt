package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.combineToPair
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.extension.ChatExtension
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatVariant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasDeclinedIncomingRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasPendingIncomingRequest
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummary
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.chatListOrder
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.withPendingMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.withRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import javax.inject.Inject

class SubscribeActiveChatsUseCase @Inject constructor(
    private val chatEngine: ChatEngine,
    private val contactsRepository: ContactsRepository,
    private val chatExtensionRegistry: ChatExtensionRegistry,
    private val chatDisplayGenerator: ChatDisplayGenerator,
    private val roomPolicyProvider: ChatRoomPolicyProvider,
    private val personChatMode: PersonChatMode,
    private val attachmentStager: ProductAttachmentStager,
    private val personChatRooms: PersonChatRooms,
) {
    context(scope: ComputationalScope)
    operator fun invoke(): Flow<List<Chat>> = combineToPair(
        chatExtensionRegistry.observeActiveExtensions(),
        // While the chat product runs the chats, the native chat's contact chats are not listed (HX2).
        if (personChatMode.isProductChat()) flowOf(emptyList()) else contactsRepository.subscribeContactsWithChatRequests(),
    ).flatMapLatest { (activeExtensions, contacts) ->
        val context = buildActiveChatsContext(activeExtensions, contacts)

        combineToPair(withRoomPolicies(chatEngine.subscribeChatSummaries()), pendingMessages())
            .map { (summariesAndPolicies, pending) ->
                val (summaries, policies) = summariesAndPolicies
                // The person chats of the chat product sort by the time their product gives; the native chat keeps its order.
                val ordered = if (personChatMode.isProductChat()) {
                    val productTimestamps = policies.mapNotNull { (chatId, policy) ->
                        val pendingAt = pending[chatId]?.createdAt
                        listOfNotNull(policy.listTimestamp, pendingAt).maxOrNull()?.let { chatId to it }
                    }.toMap()
                    chatListOrder(summaries, productTimestamps)
                } else {
                    summaries
                }
                ordered.mapNotNull { messageSummary ->
                    val chatId = messageSummary.chatId
                    if (!context.isChatActive(chatId)) return@mapNotNull null

                    val policy = policies[chatId]
                    // As the native chat lists its contacts: a request waits behind "New requests", and neither an invite
                    // nor a blocked contact shows. A request that the user sent shows at once, as the native contact does.
                    val requestSent = policy?.contact?.state == ChatContactStateModel.INVITE && pending[chatId]?.isFailed == false
                    if (policy?.contact != null && !policy.isListedContactRoom && !requestSent) return@mapNotNull null
                    // A room of the chat product is a person chat: it shows once its contact is declared.
                    if (policy?.contact == null && personChatRooms.isPersonChatRoom(chatId)) return@mapNotNull null

                    // As the native list shows a message it sends at once, a room shows its newest pending message.
                    val summary = messageSummary.withRoomPolicy(policy).withPendingMessage(pending[chatId], policy, attachmentStager::preview)
                    val display = constructChatDisplay(chatId, summary, policy, context.activeContactsById) ?: return@mapNotNull null

                    Chat(
                        id = chatId,
                        display = display,
                        preview = summary.preview,
                        timestamp = summary.timestamp,
                        unreadBadge = summary.badge,
                        hasUnseenReaction = summary.hasUnseenReaction,
                        customPreviewRenderer = summary.customPreviewRenderer
                    )
                }
            }
    }.inBackground()

    private fun pendingMessages(): Flow<Map<ChatId, ProductOutboxEntry>> =
        if (personChatMode.isProductChat()) attachmentStager.observeNewest() else flowOf(emptyMap())

    context(scope: ComputationalScope)
    private fun withRoomPolicies(summaries: Flow<List<ChatSummary>>): Flow<Pair<List<ChatSummary>, Map<ChatId, ChatRoomPolicy>>> {
        val sharedSummaries = summaries.shareIn(scope, SharingStarted.WhileSubscribed(), replay = 1)
        val policies = sharedSummaries
            .map { list -> list.mapToSet { it.chatId } }
            .distinctUntilChanged()
            .flatMapLatest { roomPolicyProvider.observePolicies(it) }

        return combineToPair(sharedSummaries, policies)
    }

    private fun buildActiveChatsContext(
        activeExtensions: List<ChatExtension>,
        contacts: List<ContactWithChatRequest>
    ): ActiveChatsContext {
        val activeExtensionIds = activeExtensions.map { it.id }

        val contactByAccountIds = contacts
            .filter {
                !it.hasPendingIncomingRequest() &&
                    !it.hasDeclinedIncomingRequest() &&
                    !it.contact.isBlocked
            }
            .associateBy { it.contact.accountId }

        return ActiveChatsContext(
            activeExtensions = activeExtensionIds,
            activeContactsById = contactByAccountIds
        )
    }

    private suspend fun constructChatDisplay(
        chatId: ChatId,
        summary: ChatSummary,
        policy: ChatRoomPolicy?,
        contactsById: Map<AccountId, ContactWithChatRequest>,
    ): ChatDisplay? {
        // A room of the chat product shows its contact, as the native chat shows a contact.
        policy?.contact?.let { return chatEngine.getContactRoomDisplay(it) }

        val defaultDisplay = constructDefaultChatDisplay(chatId, contactsById) ?: return null

        return chatDisplayGenerator.applyRoomMetadata(summary.roomMetadata, defaultDisplay)
    }

    private suspend fun constructDefaultChatDisplay(
        chatId: ChatId,
        contactsById: Map<AccountId, ContactWithChatRequest>,
    ): ChatDisplay? {
        return when (val chatVariant = chatId.chatVariant()) {
            is ChatVariant.Extension -> chatEngine.getDefaultChatExtensionDisplay(chatId)
                .logFailure("Failed to load chat summary")
                .getOrNull()

            is ChatVariant.Contact -> {
                val contact = contactsById[chatVariant.contactAccountId] ?: return null
                chatEngine.getContactChatDisplay(contact.contact)
            }
        }
    }

    private data class ActiveChatsContext(
        val activeExtensions: List<ChatExtensionId>,
        val activeContactsById: Map<AccountId, ContactWithChatRequest>
    ) {
        fun isChatActive(chatId: ChatId): Boolean {
            return when (val variant = chatId.chatVariant()) {
                is ChatVariant.Contact -> variant.contactAccountId in activeContactsById
                is ChatVariant.Extension -> variant.extensionId in activeExtensions
            }
        }
    }
}
