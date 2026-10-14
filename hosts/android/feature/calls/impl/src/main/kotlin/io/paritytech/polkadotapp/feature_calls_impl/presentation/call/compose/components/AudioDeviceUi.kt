package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.AudioFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.CallOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.Headphones
import io.paritytech.polkadotapp.design.components.icon.vectors.PhoneBluetoothSpeaker
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioDeviceType
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.AudioDeviceUiModel
import io.paritytech.polkadotapp.common.R as RCommon

fun audioDeviceIcon(type: CallAudioDeviceType?): ImageVector = when (type) {
    CallAudioDeviceType.Speaker -> NovaIcons.AudioFilled
    CallAudioDeviceType.Bluetooth -> NovaIcons.PhoneBluetoothSpeaker
    CallAudioDeviceType.WiredHeadset,
    CallAudioDeviceType.Usb -> NovaIcons.Headphones
    CallAudioDeviceType.Earpiece, null -> NovaIcons.CallOutlined
}

@Composable
fun audioDeviceLabel(device: AudioDeviceUiModel): String = when (device.type) {
    CallAudioDeviceType.Earpiece -> stringResource(RCommon.string.call_audio_device_phone)
    CallAudioDeviceType.Speaker -> stringResource(RCommon.string.call_audio_device_speaker)
    CallAudioDeviceType.WiredHeadset,
    CallAudioDeviceType.Usb -> device.name ?: stringResource(RCommon.string.call_audio_device_headphones)
    CallAudioDeviceType.Bluetooth -> device.name ?: stringResource(RCommon.string.call_audio_device_bluetooth)
}
