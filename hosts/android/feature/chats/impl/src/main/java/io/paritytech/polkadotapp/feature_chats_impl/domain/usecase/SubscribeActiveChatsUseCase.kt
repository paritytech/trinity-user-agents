package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.combineToPair
import io.paritytech.polkadotapp.common.utils.inBackground
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatContactStateModel
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull
import io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox.ProductOutboxEntry
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatEngine
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummary
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.chatListOrder
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.withPayments
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.withPendingMessage
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.withRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPayment
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.payments.ProductChatPaymentStore
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicyProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.shareIn
import javax.inject.Inject

class SubscribeActiveChatsUseCase @Inject constructor(
    private val chatEngine: ChatEngine,
    private val chatExtensionRegistry: ChatExtensionRegistry,
    private val chatDisplayGenerator: ChatDisplayGenerator,
    private val roomPolicyProvider: ChatRoomPolicyProvider,
    private val attachmentStager: ProductAttachmentStager,
    private val personChatRooms: PersonChatRooms,
    private val productPayments: ProductChatPaymentStore,
) {
    context(scope: ComputationalScope)
    operator fun invoke(): Flow<List<Chat>> = chatExtensionRegistry.observeActiveExtensions().flatMapLatest { activeExtensions ->
        val activeExtensionIds = activeExtensions.mapToSet { it.id }

        combine(withRoomPolicies(chatEngine.subscribeChatSummaries()), pendingMessages(), roomPayments(), ::Triple)
            .map { (summariesAndPolicies, pending, payments) ->
                val (summaries, policies) = summariesAndPolicies
                // The person chats of the chat product sort by the time their product gives.
                val productTimestamps = policies.mapNotNull { (chatId, policy) ->
                    val pendingAt = pending[chatId]?.createdAt
                    val paymentAt = payments[chatId]?.filter { !it.incoming && it.isUndelivered }?.maxOfOrNull { it.timestamp }
                    listOfNotNull(policy.listTimestamp, pendingAt, paymentAt).maxOrNull()?.let { chatId to it }
                }.toMap()
                chatListOrder(summaries, productTimestamps).mapNotNull { messageSummary ->
                    val chatId = messageSummary.chatId
                    // A contact chat id is left over from the native chat; its chat is a room of the chat product.
                    val extensionId = chatId.extensionOrNull()?.extensionId ?: return@mapNotNull null
                    if (extensionId !in activeExtensionIds) return@mapNotNull null

                    val policy = policies[chatId]
                    // As the native chat lists its contacts: a request waits behind "New requests", and neither an invite
                    // nor a blocked contact shows. A request that the user sent shows at once, as the native contact does.
                    val requestSent = policy?.contact?.state == ChatContactStateModel.INVITE && pending[chatId]?.isFailed == false
                    if (policy?.contact != null && !policy.isListedContactRoom && !requestSent) return@mapNotNull null
                    // A room of the chat product is a person chat: it shows once its contact is declared.
                    if (policy?.contact == null && personChatRooms.isPersonChatRoom(chatId)) return@mapNotNull null

                    // As the native list shows a message it sends at once, a room shows its newest pending message.
                    val summary = messageSummary.withRoomPolicy(policy)
                        .withPendingMessage(pending[chatId], policy, attachmentStager::preview)
                        .withPayments(payments[chatId].orEmpty(), policy)
                    val display = constructChatDisplay(chatId, summary, policy) ?: return@mapNotNull null

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

    private fun pendingMessages(): Flow<Map<ChatId, ProductOutboxEntry>> = attachmentStager.observeNewest()

    private fun roomPayments(): Flow<Map<ChatId, List<ProductChatPayment>>> =
        productPayments.observeAll().map { all -> all.groupBy { it.chatId } }

    context(scope: ComputationalScope)
    private fun withRoomPolicies(summaries: Flow<List<ChatSummary>>): Flow<Pair<List<ChatSummary>, Map<ChatId, ChatRoomPolicy>>> {
        val sharedSummaries = summaries.shareIn(scope, SharingStarted.WhileSubscribed(), replay = 1)
        val policies = sharedSummaries
            .map { list -> list.mapToSet { it.chatId } }
            .distinctUntilChanged()
            .flatMapLatest { roomPolicyProvider.observePolicies(it) }

        return combineToPair(sharedSummaries, policies)
    }

    private suspend fun constructChatDisplay(
        chatId: ChatId,
        summary: ChatSummary,
        policy: ChatRoomPolicy?,
    ): ChatDisplay? {
        // A room of the chat product shows its contact, as the native chat shows a contact.
        policy?.contact?.let { return chatEngine.getContactRoomDisplay(it) }

        val defaultDisplay = chatEngine.getDefaultChatExtensionDisplay(chatId)
            .logFailure("Failed to load chat summary")
            .getOrNull() ?: return null

        return chatDisplayGenerator.applyRoomMetadata(summary.roomMetadata, defaultDisplay)
    }
}
