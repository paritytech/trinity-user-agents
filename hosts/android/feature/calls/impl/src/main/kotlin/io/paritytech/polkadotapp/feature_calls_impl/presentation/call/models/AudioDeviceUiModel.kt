package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioDeviceType

@Immutable
data class AudioDeviceUiModel(
    val id: Int,
    val type: CallAudioDeviceType,
    val name: String?,
    val isSelected: Boolean,
)
