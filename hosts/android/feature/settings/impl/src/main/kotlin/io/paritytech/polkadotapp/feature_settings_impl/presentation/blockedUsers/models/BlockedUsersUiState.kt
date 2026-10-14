package io.paritytech.polkadotapp.feature_settings_impl.presentation.blockedUsers.models

import androidx.compose.runtime.Immutable
import kotlinx.collections.immutable.ImmutableList

@Immutable
data class BlockedUsersUiState(
    val blockedUsers: ImmutableList<BlockedUserUiModel>
)
