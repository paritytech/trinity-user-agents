package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel

/** How long the user mutes a product room (07-LATER §6.2). */
enum class ChatMuteDuration(private val durationMs: Long?) {
    ONE_HOUR(HOUR_MS),
    EIGHT_HOURS(8 * HOUR_MS),
    ONE_WEEK(7 * 24 * HOUR_MS),

    /** The mute has no end. */
    ALWAYS(null);

    /** The end of a mute that starts at [nowMs], in milliseconds since the Unix epoch. */
    fun mutedUntil(nowMs: Long): Long = durationMs?.let { nowMs + it } ?: ChatRoomUserStateModel.MUTED_WITHOUT_END
}

private const val HOUR_MS = 60L * 60L * 1000L

/**
 * Changes the mute and the archive of a product room, and tells the product of the room through
 * `UserStateChanged`. Only an action of the user calls it. A person chat has no mute and no archive (D34).
 */
interface ChatRoomUserStateService {
    suspend fun mute(chatId: ChatId, duration: ChatMuteDuration): Result<Unit>

    suspend fun unmute(chatId: ChatId): Result<Unit>

    suspend fun setArchived(chatId: ChatId, archived: Boolean): Result<Unit>
}
