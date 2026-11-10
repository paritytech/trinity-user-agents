package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.isPublicProfileAvatarUri
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay

class ChatDisplayUiModel(
    val username: String,
    val avatarModel: AvatarUiModel
)

fun ChatDisplay.toUi(refreshAvatar: Boolean = false): ChatDisplayUiModel {
    return ChatDisplayUiModel(
        username = name,
        avatarModel = avatar.toUi(refresh = refreshAvatar)
    )
}

fun ChatAvatar.toUi(refresh: Boolean = false): AvatarUiModel {
    return when (this) {
        is ChatAvatar.Account -> toNameUi()
        is ChatAvatar.Url -> AvatarUiModel.Image(
            url,
            placeholder = placeholder?.toNameUi(),
            refresh = refresh && isPublicProfileAvatarUri(url)
        )
    }
}

private fun ChatAvatar.Account.toNameUi() = AvatarUiModel.Name(name, AvatarColorScheme.from(themeSeed))
