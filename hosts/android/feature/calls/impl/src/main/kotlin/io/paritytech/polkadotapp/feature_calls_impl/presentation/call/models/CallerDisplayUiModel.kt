package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models

import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel

data class CallerDisplayUiModel(
    val name: String,
    val avatar: AvatarUiModel?,
) {
    companion object {
        val Empty = CallerDisplayUiModel(name = "", avatar = null)
    }
}
