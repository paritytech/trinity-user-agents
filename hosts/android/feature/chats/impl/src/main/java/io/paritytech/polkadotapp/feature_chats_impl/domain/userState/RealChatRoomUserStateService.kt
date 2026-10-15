@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_chats_impl.domain.userState

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomUserStateService
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.ChatRoomUserStates
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.ExperimentalTime

/**
 * Applies one change of the user to the stored state of a room. An action that changes nothing does not tell
 * the product. A mute that ended is removed with the next change, so the product does not get an old end time.
 */
@Singleton
class RealChatRoomUserStateService @Inject constructor(
    private val repository: ChatRoomUserStateRepository,
    private val userStates: ChatRoomUserStates,
    private val timeProvider: TimeProvider,
) : ChatRoomUserStateService {
    // Two quick taps must not read the same old state, so one change completes before the next starts.
    private val mutex = Mutex()

    override suspend fun mute(chatId: ChatId, duration: ChatMuteDuration): Result<Unit> =
        update(chatId) { now -> copy(mutedUntil = duration.mutedUntil(now)) }

    override suspend fun unmute(chatId: ChatId): Result<Unit> = update(chatId) { copy(mutedUntil = null) }

    override suspend fun setArchived(chatId: ChatId, archived: Boolean): Result<Unit> =
        update(chatId) { copy(archived = archived) }

    private suspend fun update(
        chatId: ChatId,
        change: ChatRoomUserStateModel.(nowMs: Long) -> ChatRoomUserStateModel,
    ): Result<Unit> = mutex.withLock {
        val now = timeProvider.now().toEpochMilliseconds()
        val stored = repository.observe(chatId).first()
        val current = if (stored.isMutedAt(now)) stored else stored.copy(mutedUntil = null)
        val next = current.change(now)

        if (next == stored) Result.success(Unit) else userStates.set(chatId, next)
    }
}
