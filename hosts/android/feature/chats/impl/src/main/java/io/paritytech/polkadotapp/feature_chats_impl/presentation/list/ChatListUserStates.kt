package io.paritytech.polkadotapp.feature_chats_impl.presentation.list

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.models.ChatListUiState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.transformLatest

// The mute and the archive of the product rooms in the chat list (07-LATER §6.2).

/** The rooms of the main section and the archived rooms, each in the order of the list. */
internal data class ChatListSections(val main: List<Chat>, val archived: List<Chat>)

/** An archived room stays archived when new messages arrive, so only the stored state moves it. */
internal fun List<Chat>.toSections(): ChatListSections {
    val (archived, main) = partition { it.roomPolicy?.userState?.archived == true }
    return ChatListSections(main = main, archived = archived)
}

/** Null for a person chat: the user mutes and archives only product rooms (D34). */
internal fun Chat.userStateAt(nowMs: Long): ChatListUiState.UserState? {
    val state = roomPolicy?.userState ?: return null
    return ChatListUiState.UserState(isMuted = state.isMutedAt(nowMs), isArchived = state.archived)
}

/** The first time after [nowMs] at which a mute of [chats] ends, or null. A mute without an end never ends. */
internal fun nextMuteEnd(chats: List<Chat>, nowMs: Long): Long? = chats
    .mapNotNull { it.roomPolicy?.userState?.mutedUntil }
    .filter { it > nowMs && it != ChatRoomUserStateModel.MUTED_WITHOUT_END }
    .minOrNull()

/**
 * Gives each list of chats with the time of the list. When a mute ends, the same list comes again with the new time,
 * so that the row loses its mute mark without a change in the database.
 */
@OptIn(ExperimentalCoroutinesApi::class)
internal fun Flow<List<Chat>>.withMuteClock(now: () -> Long): Flow<Pair<List<Chat>, Long>> = transformLatest { chats ->
    while (true) {
        val time = now()
        emit(chats to time)
        val next = nextMuteEnd(chats, time) ?: break
        delay(next - time)
    }
}
