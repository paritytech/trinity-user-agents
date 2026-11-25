package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay

class ChatDisplayUiModel(
    val username: String,
    val avatarModel: AvatarUiModel
)

fun ChatDisplay.toUi(forceFresh: Boolean): ChatDisplayUiModel {
    return ChatDisplayUiModel(
        username = name,
        avatarModel = avatar.toUi(forceFresh = forceFresh)
    )
}

fun ChatAvatar.toUi(forceFresh: Boolean): AvatarUiModel {
    return when (this) {
        is ChatAvatar.Account -> toNameUi()
        is ChatAvatar.Url -> AvatarUiModel.Image(
            url,
            placeholder = placeholder?.toNameUi()
        )
        is ChatAvatar.PublicProfile -> AvatarUiModel.PublicProfile(
            username = username,
            placeholder = placeholder.toNameUi(),
            forceFresh = forceFresh
        )
    }
}

private fun ChatAvatar.Account.toNameUi() = AvatarUiModel.Name(name, AvatarColorScheme.from(themeSeed))
