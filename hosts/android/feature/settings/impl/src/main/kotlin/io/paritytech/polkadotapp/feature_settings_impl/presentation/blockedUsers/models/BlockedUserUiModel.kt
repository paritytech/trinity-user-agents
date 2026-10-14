package io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel

@Immutable
data class BlockedUserUiModel(
    val accountId: AccountId,
    val displayName: String,
    val avatarModel: AvatarUiModel
)
