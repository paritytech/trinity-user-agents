package io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel

class UserSearchResultUiModel(
    val contactAccountId: AccountId,
    val username: String,
    val avatarModel: AvatarUiModel,
)
