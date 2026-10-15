package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.call

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomCallModel
import io.paritytech.polkadotapp.feature_chats_impl.data.declarations.RealChatRoomCallRepository
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/** The chat-list row of a room that rings shows "Incoming call" as an activity that continues (07-LATER §11.2). */
class ProductRoomCallListPreviews @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val calls: RealChatRoomCallRepository,
) {
    /** The preview text of each room that rings. */
    fun observe(): Flow<Map<ChatId, String>> = calls.observeAll()
        .map { all ->
            all.mapNotNull { (chatId, call) -> (call as? ChatRoomCallModel.Ringing)?.let { chatId to ringingTitle(it.video) } }.toMap()
        }
        .distinctUntilChanged()

    fun ringingTitle(video: Boolean): String = context.getString(ringingTitleRes(video))
}

/** "Incoming call" or "Incoming video call". */
internal fun ringingTitleRes(video: Boolean): Int =
    if (video) RCommon.string.product_room_call_incoming_video else RCommon.string.product_room_call_incoming
