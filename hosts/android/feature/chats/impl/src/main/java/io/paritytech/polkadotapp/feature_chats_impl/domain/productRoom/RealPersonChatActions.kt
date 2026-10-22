package io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatActions
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatUserActionModel
import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ProductRoomUserActions
import javax.inject.Inject

class RealPersonChatActions @Inject constructor(
    private val userActions: ProductRoomUserActions,
) : PersonChatActions {
    override suspend fun unblock(chatId: ChatId): Result<Unit> = userActions.contactAction(chatId, ChatUserActionModel.Unblock)
}
