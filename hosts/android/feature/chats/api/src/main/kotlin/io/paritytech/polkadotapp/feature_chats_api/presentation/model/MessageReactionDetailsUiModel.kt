package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import kotlinx.collections.immutable.ImmutableList

data class EmojiReactionGroup(
    val emoji: String,
    val reactions: ImmutableList<ReactionDetail>
)

data class ReactionDetail(
    val emoji: String,
    val timestamp: Timestamp,
    val isUser: Boolean,
    val authorName: String,
    val avatarModel: AvatarUiModel,
)
