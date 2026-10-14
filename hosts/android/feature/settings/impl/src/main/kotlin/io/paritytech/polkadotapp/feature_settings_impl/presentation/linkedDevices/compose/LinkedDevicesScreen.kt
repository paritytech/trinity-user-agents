package io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.compose

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Add
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.LinkedDevicesContract
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.compose.components.LinkedDevicesSection
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDeviceCategory
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDeviceUiModel
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDevicesSectionUiModel
import io.paritytech.polkadotapp.feature_settings_impl.presentation.linkedDevices.models.LinkedDevicesUiState
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon
import io.paritytech.polkadotapp.design.R as RDesign

@Composable
fun LinkedDevicesScreen(contract: LinkedDevicesContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    LinkedDevicesScreenInternal(
        state = state,
        onBackClick = contract::onBackClick,
        onAddDeviceClick = contract::onAddDeviceClick,
        onHowItWorksClick = contract::onHowItWorksClick,
        onDeviceClick = contract::onDeviceClick
    )
}

@Composable
private fun LinkedDevicesScreenInternal(
    state: LinkedDevicesUiState,
    onBackClick: () -> Unit,
    onAddDeviceClick: () -> Unit,
    onHowItWorksClick: () -> Unit,
    onDeviceClick: (String) -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.settings_linked_devices),
                titleAlignment = TopBarTitleAlignment.Center,
                navigationAction = rememberTopBarAction(onBackClick),
                actions = if (state.isEmpty) {
                    persistentListOf()
                } else {
                    persistentListOf(rememberTopBarAction(action = onAddDeviceClick, icon = NovaIcons.Add))
                },
            )

            if (state.isEmpty) {
                EmptyState(
                    onAddDeviceClick = onAddDeviceClick,
                    onHowItWorksClick = onHowItWorksClick
                )
            } else {
                DevicesList(
                    state = state,
                    onDeviceClick = onDeviceClick
                )
            }
        }
    }
}

@Composable
private fun EmptyState(
    onAddDeviceClick: () -> Unit,
    onHowItWorksClick: () -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .weight(1f),
            contentAlignment = Alignment.Center
        ) {
            LinkNewDeviceContent(onScanQrCodeClick = onAddDeviceClick)
        }

        BottomFooter(onHowItWorksClick = onHowItWorksClick)
    }
}

@Composable
private fun LinkNewDeviceContent(onScanQrCodeClick: () -> Unit) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.large)
    ) {
        Image(
            modifier = Modifier.size(192.dp),
            painter = painterResource(RDesign.drawable.img_linked_devices_computer),
            contentDescription = null
        )

        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)
        ) {
            NovaText(
                text = stringResource(RCommon.string.linked_devices_link_new_title),
                color = PolkadotTheme.colors.fg.primary,
                style = PolkadotTheme.typography.title.large,
                textAlign = TextAlign.Center
            )

            NovaText(
                text = stringResource(RCommon.string.linked_devices_link_new_description),
                color = PolkadotTheme.colors.fg.secondary,
                style = PolkadotTheme.typography.body.medium,
                textAlign = TextAlign.Center
            )
        }

        PolkadotTextButton(
            text = stringResource(RCommon.string.linked_devices_scan_qr_action),
            onClick = onScanQrCodeClick
        )
    }
}

@Composable
private fun BottomFooter(onHowItWorksClick: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(bottom = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.large)
    ) {
        NovaText(
            text = stringResource(RCommon.string.linked_devices_safety_note),
            color = PolkadotTheme.colors.fg.tertiary,
            style = PolkadotTheme.typography.body.medium,
            textAlign = TextAlign.Center
        )
    }
}

@Composable
private fun DevicesList(
    state: LinkedDevicesUiState,
    onDeviceClick: (String) -> Unit
) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.large)
    ) {
        state.sections.forEach { section ->
            LinkedDevicesSection(
                section = section,
                onDeviceClick = onDeviceClick
            )
        }
    }
}

@Preview
@Composable
private fun LinkedDevicesScreenEmptyPreview() {
    PolkadotTheme {
        LinkedDevicesScreenInternal(
            state = LinkedDevicesUiState(),
            onBackClick = {},
            onAddDeviceClick = {},
            onHowItWorksClick = {},
            onDeviceClick = {}
        )
    }
}

@Preview
@Composable
private fun LinkedDevicesScreenListPreview() {
    PolkadotTheme {
        LinkedDevicesScreenInternal(
            state = LinkedDevicesUiState(
                sections = persistentListOf(
                    LinkedDevicesSectionUiModel(
                        category = LinkedDeviceCategory.LAPTOP_DESKTOP,
                        devices = persistentListOf(
                            LinkedDeviceUiModel("1", "MacBook Pro 16", "Polkadot Desktop v.0.2.13", LinkedDeviceCategory.LAPTOP_DESKTOP),
                            LinkedDeviceUiModel("2", "Windows 11", "Polkadot Desktop v.1.2.13", LinkedDeviceCategory.LAPTOP_DESKTOP),
                            LinkedDeviceUiModel("3", "iMac 2024", "Polkadot Desktop v.0.2.13", LinkedDeviceCategory.LAPTOP_DESKTOP)
                        )
                    )
                )
            ),
            onBackClick = {},
            onAddDeviceClick = {},
            onHowItWorksClick = {},
            onDeviceClick = {}
        )
    }
}
