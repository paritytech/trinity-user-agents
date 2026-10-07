package io.paritytech.polkadotapp.feature_chats_impl.data.mappers

import io.paritytech.polkadotapp.database.dao.ChatMessageDao
import io.paritytech.polkadotapp.feature_chats_impl.domain.ChatMessagePlacement

internal fun ChatMessagePlacement.toLocal(): ChatMessageDao.Placement {
    return when (this) {
        ChatMessagePlacement.Latest -> ChatMessageDao.Placement.Latest
        ChatMessagePlacement.ByTimestamp -> ChatMessageDao.Placement.ByTimestamp
        is ChatMessagePlacement.SameAs -> ChatMessageDao.Placement.SameAs(messageId)
    }
}
