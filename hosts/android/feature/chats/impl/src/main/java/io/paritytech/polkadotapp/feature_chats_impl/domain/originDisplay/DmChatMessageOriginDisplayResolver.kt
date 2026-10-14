package io.paritytech.polkadotapp.feature_chats_impl.domain.originDisplay

import android.content.Context
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.MessageOriginDisplay
import io.paritytech.polkadotapp.common.R as RCommon

class DmChatMessageOriginDisplayResolver @AssistedInject constructor(
    @Assisted private val chatDisplay: ChatDisplay,
    @Assisted private val ourAccountId: AccountId,
    @ApplicationContext private val context: Context
) : MessageOriginDisplayResolver {
    @AssistedFactory
    interface Factory {
        fun create(
            chatDisplay: ChatDisplay,
            ourAccountId: AccountId
        ): DmChatMessageOriginDisplayResolver
    }

    override fun displayOf(messageOrigin: ChatMessageOrigin): MessageOriginDisplay {
        return when (messageOrigin) {
            is ChatMessageOrigin.Contact,
            is ChatMessageOrigin.Extension -> chatDisplay.toOriginDisplay()

            ChatMessageOrigin.User -> {
                val name = context.getString(RCommon.string.common_you)
                MessageOriginDisplay(
                    name = name,
                    avatar = ChatAvatar.Account(name, ourAccountId.value)
                )
            }
        }
    }

    private fun ChatDisplay.toOriginDisplay(): MessageOriginDisplay {
        return MessageOriginDisplay(name, avatar)
    }
}
