@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Check
import io.paritytech.polkadotapp.design.components.menu.PolkadotMenuList
import io.paritytech.polkadotapp.design.components.menu.PolkadotMenuListItem
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioDeviceType
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.AudioDeviceUiModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

@Composable
fun CallAudioDeviceSheet(
    isVisible: Boolean,
    devices: ImmutableList<AudioDeviceUiModel>,
    onSelect: (Int) -> Unit,
    onDismiss: () -> Unit,
) {
    NovaModalBottomSheet(
        isVisible = isVisible,
        onDismissRequest = onDismiss,
    ) {
        AudioDeviceSheetContent(
            devices = devices,
            onSelect = onSelect,
        )
    }
}

@Composable
private fun AudioDeviceSheetContent(
    devices: ImmutableList<AudioDeviceUiModel>,
    onSelect: (Int) -> Unit,
) {
    PolkadotMenuList {
        devices.forEach { device ->
            PolkadotMenuListItem(
                leading = {
                    NovaIcon(
                        modifier = Modifier.size(32.dp),
                        imageVector = audioDeviceIcon(device.type),
                    )
                },
                trailing = if (device.isSelected) {
                    {
                        NovaIcon(
                            modifier = Modifier.size(32.dp),
                            imageVector = NovaIcons.Check,
                        )
                    }
                } else {
                    null
                },
                title = { NovaText(audioDeviceLabel(device)) },
                onClick = { onSelect(device.id) },
            )
        }
    }
}

@Preview(backgroundColor = 0xFF191919, showBackground = true)
@Composable
private fun AudioDeviceSheetContentPreview() {
    PolkadotTheme {
        AudioDeviceSheetContent(
            devices = persistentListOf(
                AudioDeviceUiModel(id = 1, type = CallAudioDeviceType.Earpiece, name = null, isSelected = false),
                AudioDeviceUiModel(id = 2, type = CallAudioDeviceType.Speaker, name = null, isSelected = false),
                AudioDeviceUiModel(id = 3, type = CallAudioDeviceType.Bluetooth, name = "Galaxy Buds", isSelected = true),
                AudioDeviceUiModel(id = 4, type = CallAudioDeviceType.WiredHeadset, name = null, isSelected = false),
                AudioDeviceUiModel(id = 5, type = CallAudioDeviceType.Usb, name = "USB-C DAC", isSelected = false),
            ),
            onSelect = {},
        )
    }
}
