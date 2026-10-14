package io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.compose

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Trash
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.DeviceDetailsContract
import io.paritytech.polkadotapp.feature_settings_impl.presentation.deviceDetails.models.DeviceDetailsUiState
import io.paritytech.polkadotapp.common.R as RCommon
import io.paritytech.polkadotapp.design.R as RDesign

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DeviceDetailsScreen(contract: DeviceDetailsContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    DeviceDetailsScreenInternal(
        state = state,
        onBackClick = contract::onBackClick,
        onRemoveDeviceClick = contract::onRemoveDeviceClick,
    )

    val loadedState = (state as? LoadingState.Loaded)?.data
    NovaModalBottomSheet(
        isVisible = loadedState?.isRemoveConfirmationVisible == true,
        onDismissRequest = contract::onCancelRemoveClick,
    ) {
        if (loadedState != null) {
            RemoveDeviceSheetContent(
                state = loadedState,
                isRemoving = loadedState.isRemoving,
                onCancelClick = contract::onCancelRemoveClick,
                onConfirmClick = contract::onConfirmRemoveClick,
            )
        }
    }
}

@Composable
private fun DeviceDetailsScreenInternal(
    state: LoadingState<DeviceDetailsUiState>,
    onBackClick: () -> Unit,
    onRemoveDeviceClick: () -> Unit,
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.device_details_toolbar_title),
                titleAlignment = TopBarTitleAlignment.Center,
                navigationAction = rememberTopBarAction(onBackClick),
            )

            val uiState = (state as? LoadingState.Loaded)?.data ?: return@Column

            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
            ) {
                VerticalSpacer { mediumIncreased }

                DeviceInfoCard(state = uiState)

                RemoveDeviceCard(
                    enabled = !uiState.isRemoving,
                    onClick = onRemoveDeviceClick,
                )
            }
        }
    }
}

@Composable
private fun DeviceInfoCard(state: DeviceDetailsUiState) {
    val placeholder = stringResource(RCommon.string.device_details_field_placeholder)

    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        color = PolkadotTheme.colors.bg.surface.container,
        shape = PolkadotTheme.shapes.mediumIncreased,
    ) {
        Column(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        ) {
            DeviceInfoRow(
                label = stringResource(RCommon.string.device_details_field_device),
                value = state.deviceLabel ?: placeholder,
                showDivider = true,
            )
            DeviceInfoRow(
                label = stringResource(RCommon.string.device_details_field_host),
                value = state.hostLabel ?: placeholder,
                showDivider = true,
            )
            DeviceInfoRow(
                label = stringResource(RCommon.string.device_details_field_added),
                value = state.addedLabel ?: placeholder,
                showDivider = false,
            )
        }
    }
}

@Composable
private fun DeviceInfoRow(
    label: String,
    value: String,
    showDivider: Boolean,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(52.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = label,
            color = PolkadotTheme.colors.fg.primary,
            style = PolkadotTheme.typography.body.large,
        )

        HorizontalSpacer { small }

        NovaText(
            text = value,
            color = PolkadotTheme.colors.fg.tertiary,
            style = PolkadotTheme.typography.body.medium,
            overflow = TextOverflow.Ellipsis
        )
    }

    if (showDivider) {
        HorizontalDivider(
            thickness = 1.dp,
            color = PolkadotTheme.colors.stroke.primary,
        )
    }
}

@Composable
private fun RemoveDeviceCard(
    enabled: Boolean,
    onClick: () -> Unit,
) {
    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        color = PolkadotTheme.colors.bg.surface.container,
        shape = PolkadotTheme.shapes.mediumIncreased,
        enabled = enabled,
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(52.dp)
                .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
        ) {
            NovaIcon(
                modifier = Modifier.size(24.dp),
                imageVector = NovaIcons.Trash,
                tint = PolkadotTheme.colors.fg.error,
            )

            NovaText(
                text = stringResource(RCommon.string.device_details_remove_action),
                color = PolkadotTheme.colors.fg.error,
                style = PolkadotTheme.typography.body.large,
            )
        }
    }
}

@Composable
private fun RemoveDeviceSheetContent(
    state: DeviceDetailsUiState,
    isRemoving: Boolean,
    onCancelClick: () -> Unit,
    onConfirmClick: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.mediumIncreased,
                top = PolkadotTheme.spacings.extraLargeIncreased,
                bottom = PolkadotTheme.spacings.large,
            ),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraLarge)
    ) {
        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.device_details_remove_confirm_title),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = TextAlign.Center,
        )

        Column(
            modifier = Modifier.fillMaxWidth(),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
        ) {
            Image(
                modifier = Modifier.size(width = 80.dp, height = 68.dp),
                painter = painterResource(RDesign.drawable.img_link_device_pixel_computer),
                contentScale = ContentScale.Inside,
                colorFilter = ColorFilter.tint(PolkadotTheme.colors.fg.primary),
                contentDescription = null
            )

            NovaText(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.device_details_remove_confirm_message_format, rememberRemoveSubject(state)),
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary,
                textAlign = TextAlign.Center,
            )
        }

        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
        ) {
            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.device_details_remove_confirm_positive),
                style = PolkadotButtonStyle.destructive(),
                shape = PolkadotButtonShape.pill,
                loading = isRemoving,
                onClick = onConfirmClick,
            )

            PolkadotTextButton(
                modifier = Modifier.fillMaxWidth(),
                text = stringResource(RCommon.string.device_details_remove_confirm_negative),
                style = PolkadotButtonStyle.ghost(),
                shape = PolkadotButtonShape.pill,
                enabled = !isRemoving,
                onClick = onCancelClick,
            )
        }
    }
}

@Composable
private fun rememberRemoveSubject(state: DeviceDetailsUiState): String {
    val host = state.hostLabel?.takeIf { it.isNotBlank() }
    val device = state.deviceLabel?.takeIf { it.isNotBlank() }

    return when {
        host != null && device != null ->
            stringResource(RCommon.string.device_details_remove_confirm_subject_with_device, host, device)

        host != null -> host
        device != null -> device
        else -> stringResource(RCommon.string.device_details_remove_confirm_subject_fallback)
    }
}

@Preview
@Composable
private fun DeviceDetailsScreenPreview() {
    PolkadotTheme {
        DeviceDetailsScreenInternal(
            state = LoadingState.Loaded(
                DeviceDetailsUiState(
                    deviceLabel = "MacBook Pro 16",
                    hostLabel = "Polkadot Desktop v.0.2.13",
                    addedLabel = "Today",
                )
            ),
            onBackClick = {},
            onRemoveDeviceClick = {},
        )
    }
}

@Preview
@Composable
private fun RemoveDeviceSheetContentPreview() {
    PolkadotTheme {
        RemoveDeviceSheetContent(
            state = DeviceDetailsUiState(
                deviceLabel = "MacBook Pro 16",
                hostLabel = "Polkadot Desktop v.0.2.13",
                addedLabel = "Today",
            ),
            isRemoving = false,
            onCancelClick = {},
            onConfirmClick = {},
        )
    }
}

@Preview
@Composable
private fun RemoveDeviceSheetContentRemovingPreview() {
    PolkadotTheme {
        RemoveDeviceSheetContent(
            state = DeviceDetailsUiState(
                deviceLabel = "MacBook Pro 16",
                hostLabel = "Polkadot Desktop v.0.2.13",
                addedLabel = "Today",
                isRemoving = true,
            ),
            isRemoving = true,
            onCancelClick = {},
            onConfirmClick = {},
        )
    }
}
