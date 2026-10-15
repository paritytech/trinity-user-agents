package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.participants

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQuerying
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipant
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import timber.log.Timber

/**
 * The participants of one product room that the room screen knows, asked from the product with `participants_query`
 * (02-DESIGN §3.3.7). The directory keeps each participant that the product resolved, so the screen asks for a
 * participant in one `ByIds` question at most. The reserved ids `user` and `product` need no question.
 */
class ChatParticipantDirectory(
    private val chatId: ChatId,
    private val scope: CoroutineScope,
    private val querying: suspend () -> ProductChatQuerying?,
) {
    private val _count = MutableStateFlow<Int?>(null)

    /** The number of participants that the product gave, or null before the answer or after a refusal. */
    val count: StateFlow<Int?> = _count.asStateFlow()

    private val _resolved = MutableStateFlow<Map<String, ChatParticipantEntryModel>>(emptyMap())
    val resolved: StateFlow<Map<String, ChatParticipantEntryModel>> = _resolved.asStateFlow()

    private val asked = mutableSetOf<String>()
    private var countJob: Job? = null

    /** Forgets the count and the participants, because the product configured the room again. */
    @Synchronized
    fun reset() {
        countJob?.cancel()
        countJob = null
        _count.value = null
        _resolved.value = emptyMap()
        asked.clear()
    }

    /** Asks the product for the number of participants once. */
    @Synchronized
    fun loadCount() {
        if (countJob != null) return

        countJob = scope.launch {
            ask { participantsCount(chatId) }
                .onSuccess { _count.value = it }
                .onFailure { Timber.d(it, "The room shows no participant count") }
        }
    }

    /** Asks the product for the participants with these ids that the directory has not asked for, in batches. */
    fun resolve(participantIds: Collection<String>) {
        val missing = claimMissing(participantIds)
        missing.chunked(BY_IDS_LIMIT).forEach { batch -> scope.launch { askByIds(batch) } }
    }

    /** As [resolve], and waits for the answers. A refused question leaves its ids unresolved. */
    suspend fun resolveNow(participantIds: Collection<String>) {
        claimMissing(participantIds).chunked(BY_IDS_LIMIT).forEach { askByIds(it) }
    }

    /** One page of the participant list, in the order of the product. A page also fills the directory. */
    suspend fun page(cursor: String?): Result<ProductParticipantsPage> =
        ask { participantsPage(chatId, cursor, PAGE_SIZE) }.onSuccess { store(it.participants) }

    @Synchronized
    private fun claimMissing(participantIds: Collection<String>): List<String> {
        val missing = participantIds
            .filter { it !in RESERVED && it !in _resolved.value && it !in asked }
            .distinct()
            .sorted()
        asked += missing
        return missing
    }

    private suspend fun askByIds(participantIds: List<String>) {
        ask { participantsByIds(chatId, participantIds) }
            .onSuccess(::store)
            .onFailure { Timber.d(it, "The room did not resolve %d participants", participantIds.size) }
    }

    private fun store(participants: List<ChatParticipantEntryModel>) {
        _resolved.update { known -> known + participants.associateBy { it.participantId } }
    }

    private suspend fun <T> ask(question: suspend ProductChatQuerying.() -> Result<T>): Result<T> {
        val querying = querying() ?: return Result.failure(ProductChatQueryException.NotAvailable())
        return querying.question()
    }

    companion object {
        /** The page size that the host asks (02-DESIGN §3.5: 1 to 200, host default 100). */
        const val PAGE_SIZE = 100

        /** The largest number of ids in one `ByIds` question (02-DESIGN §3.5). */
        const val BY_IDS_LIMIT = 100

        private val RESERVED = setOf(ProductParticipant.USER, ProductParticipant.PRODUCT)
    }
}
