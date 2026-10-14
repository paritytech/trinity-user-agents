package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay

class ChatDisplayUiModel(
    val username: String,
    val avatarModel: AvatarUiModel
)

fun ChatDisplay.toUi(): ChatDisplayUiModel {
    return ChatDisplayUiModel(
        username = name,
        avatarModel = avatar.toUi()
    )
}

fun ChatAvatar.toUi(): AvatarUiModel {
    return when (this) {
        is ChatAvatar.Account -> AvatarUiModel.Name(name, AvatarColorScheme.from(themeSeed))
        is ChatAvatar.Url -> AvatarUiModel.Image(url)
    }
}
