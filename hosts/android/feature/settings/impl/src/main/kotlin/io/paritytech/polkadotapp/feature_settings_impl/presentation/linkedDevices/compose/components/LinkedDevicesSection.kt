package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDevicesSectionUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun LinkedDevicesSection(
    modifier: Modifier = Modifier,
    section: LinkedDevicesSectionUiModel,
    onDeviceClick: (String) -> Unit
) {
    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
    ) {
        SectionHeader(
            title = stringResource(section.category.titleRes),
            count = section.devices.size
        )

        PolkadotSurface(
            shape = PolkadotTheme.shapes.mediumIncreased,
            color = PolkadotTheme.colors.bg.surface.container
        ) {
            Column {
                section.devices.forEachIndexed { index, device ->
                    LinkedDeviceItem(
                        name = device.name,
                        description = device.description,
                        onClick = { onDeviceClick(device.id) }
                    )

                    if (index < section.devices.lastIndex) {
                        HorizontalDivider(
                            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                            thickness = 1.dp,
                            color = PolkadotTheme.colors.stroke.primary
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun SectionHeader(
    title: String,
    count: Int
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.extraSmall)
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = title.uppercase(),
            color = PolkadotTheme.colors.fg.tertiary,
            style = PolkadotTheme.typography.caption.medium
        )

        NovaText(
            text = pluralStringResource(RCommon.plurals.linked_devices_count_format, count, count).uppercase(),
            color = PolkadotTheme.colors.fg.tertiary,
            style = PolkadotTheme.typography.caption.medium
        )
    }
}
