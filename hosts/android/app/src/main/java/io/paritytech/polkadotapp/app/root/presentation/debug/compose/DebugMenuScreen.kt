package io.paritytech.polkadotapp.app.root.presentation.debug.compose

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.BasicAlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.app.root.presentation.debug.DebugMenuContract
import io.paritytech.polkadotapp.app.root.presentation.debug.DebugMenuState
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.compound.NovaSwitch
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.NovaTextField
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun DebugMenuScreen(contract: DebugMenuContract) {
    val state by contract.state.collectAsStateWithLifecycle()

    DebugMenuScreenInternal(
        state = state,
        onBackClick = contract::onBackClick,
        onClearBackupClick = contract::onClearBackupClick,
        onShareLogsClick = contract::onShareLogsClick,
        onCopyWalletAccountClick = contract::onCopyWalletAccountClick,
        onCopyCandidateAccountClick = contract::onCopyCandidateAccountClick,
        onCopyWalletMnemonicClick = contract::onCopyWalletMnemonicClick,
        onOpenVideoGameClick = contract::onOpenVideoGameClick,
        onProductBotsClick = contract::onProductBotsClick,
        onPocketFacePreviewClick = contract::onPocketFacePreviewClick,
        onRandomizeAccountClick = contract::onRandomizeAccountClick,
        onOpenSpaBrowserClick = contract::onOpenSpaBrowserClick,
        onSpaBrowserUrlEntered = contract::onSpaBrowserUrlEntered,
        onSpaBrowserDialogDismissed = contract::onSpaBrowserDialogDismissed,
        onOpenDevServerClick = contract::onOpenDevServerClick,
        onDevServerAddressEntered = contract::onDevServerAddressEntered,
        onDevServerDialogDismissed = contract::onDevServerDialogDismissed,
        onClearDotNsCacheClick = contract::onClearDotNsCacheClick,
        onClearJWTTokenClick = contract::onClearJWTTokenClick,
        onSimulateGameResultsClick = contract::onSimulateGameResultsClick,
        onCoinageDebugWidgetsToggled = contract::onCoinageDebugWidgetsToggled,
        onTruapiRuntimeToggled = contract::onTruapiRuntimeToggled,
        onRuntimeRestartConfirmed = contract::onRuntimeRestartConfirmed,
        onRuntimeRestartCancelled = contract::onRuntimeRestartCancelled,
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DebugMenuScreenInternal(
    state: DebugMenuState,
    onBackClick: () -> Unit,
    onClearBackupClick: () -> Unit,
    onCopyWalletAccountClick: () -> Unit,
    onCopyCandidateAccountClick: () -> Unit,
    onCopyWalletMnemonicClick: () -> Unit,
    onShareLogsClick: () -> Unit,
    onOpenVideoGameClick: () -> Unit,
    onProductBotsClick: () -> Unit,
    onPocketFacePreviewClick: () -> Unit,
    onRandomizeAccountClick: () -> Unit,
    onOpenSpaBrowserClick: () -> Unit,
    onSpaBrowserUrlEntered: (String) -> Unit,
    onSpaBrowserDialogDismissed: () -> Unit,
    onOpenDevServerClick: () -> Unit,
    onDevServerAddressEntered: (String) -> Unit,
    onDevServerDialogDismissed: () -> Unit,
    onClearDotNsCacheClick: () -> Unit,
    onClearJWTTokenClick: () -> Unit,
    onSimulateGameResultsClick: () -> Unit,
    onCoinageDebugWidgetsToggled: (Boolean) -> Unit,
    onTruapiRuntimeToggled: (Boolean) -> Unit,
    onRuntimeRestartConfirmed: () -> Unit,
    onRuntimeRestartCancelled: () -> Unit,
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
                .verticalScroll(rememberScrollState())
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.debug_menu_title),
                navigationAction = rememberTopBarAction(
                    action = onBackClick,
                    icon = NovaIcons.Close
                ),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            VerticalSpacer { large }

            DebugMenuToggleItem(
                title = stringResource(RCommon.string.debug_menu_coinage_debug_widgets),
                checked = state.coinageDebugWidgetsEnabled,
                onCheckedChange = onCoinageDebugWidgetsToggled
            )

            VerticalSpacer { large }

            DebugMenuToggleItem(
                title = stringResource(RCommon.string.debug_menu_truapi_runtime),
                checked = state.truapiRuntimeEnabled,
                onCheckedChange = onTruapiRuntimeToggled
            )

            VerticalSpacer { extraLargeIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_clear_backup),
                enabled = !state.isClearingBackup,
                onClick = onClearBackupClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_copy_deposit_account),
                enabled = true,
                onClick = onCopyWalletAccountClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_copy_candidate_account),
                enabled = true,
                onClick = onCopyCandidateAccountClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_copy_deposit_mnemonic),
                enabled = true,
                onClick = onCopyWalletMnemonicClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_share_logs),
                enabled = !state.isSharingLogs,
                onClick = onShareLogsClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_open_video_game),
                enabled = !state.isSharingLogs,
                onClick = onOpenVideoGameClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_product_bots),
                enabled = true,
                onClick = onProductBotsClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_pocket_face_preview),
                enabled = true,
                onClick = onPocketFacePreviewClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_open_spa_browser),
                enabled = true,
                onClick = onOpenSpaBrowserClick
            )

            VerticalSpacer { mediumIncreased }

            if (state.devServerAvailable) {
                DebugMenuItem(
                    title = stringResource(RCommon.string.debug_menu_open_dev_server),
                    enabled = true,
                    onClick = onOpenDevServerClick
                )

                VerticalSpacer { mediumIncreased }
            }

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_randomize_account),
                enabled = true,
                onClick = onRandomizeAccountClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_clear_dotns_cache),
                enabled = true,
                onClick = onClearDotNsCacheClick
            )

            VerticalSpacer { mediumIncreased }

            DebugMenuItem(
                title = stringResource(
                    if (state.hasJWTToken) RCommon.string.debug_menu_clear_jwt_token_stored
                    else RCommon.string.debug_menu_clear_jwt_token
                ),
                enabled = state.hasJWTToken,
                onClick = onClearJWTTokenClick
            )

            VerticalSpacer { large }

            DebugMenuItem(
                title = stringResource(RCommon.string.debug_menu_simulate_game_results),
                enabled = true,
                onClick = onSimulateGameResultsClick
            )

            VerticalSpacer { extraLargeIncreased }
        }
    }

    if (state.showSpaBrowserDialog) {
        UrlDialog(
            title = stringResource(RCommon.string.debug_spa_browser_title),
            hint = stringResource(RCommon.string.debug_spa_browser_url_hint),
            onDismiss = onSpaBrowserDialogDismissed,
            onConfirm = onSpaBrowserUrlEntered,
        )
    }

    if (state.showDevServerDialog) {
        UrlDialog(
            title = stringResource(RCommon.string.debug_dev_server_title),
            hint = stringResource(RCommon.string.debug_dev_server_address_hint),
            onDismiss = onDevServerDialogDismissed,
            onConfirm = onDevServerAddressEntered,
        )
    }

    if (state.runtimeRestartRevertsTo != null) {
        RuntimeRestartDialog(
            onDismiss = onRuntimeRestartCancelled,
            onConfirm = onRuntimeRestartConfirmed,
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RuntimeRestartDialog(
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    BasicAlertDialog(onDismissRequest = onDismiss) {
        PolkadotSurface(
            color = PolkadotTheme.colors.bg.surface.container,
            shape = RoundedCornerShape(28.dp),
            shadowElevation = 5.dp
        ) {
            Column(
                modifier = Modifier.padding(PolkadotTheme.spacings.large),
            ) {
                NovaText(
                    text = stringResource(RCommon.string.debug_menu_runtime_restart_title),
                    style = PolkadotTheme.typography.headline.small,
                    color = PolkadotTheme.colors.fg.primary
                )

                VerticalSpacer { mediumIncreased }

                NovaText(
                    text = stringResource(RCommon.string.debug_menu_runtime_restart_message),
                    style = PolkadotTheme.typography.body.large,
                    color = PolkadotTheme.colors.fg.secondary
                )

                VerticalSpacer { large }

                Row(modifier = Modifier.align(Alignment.End)) {
                    PolkadotTextButton(
                        text = stringResource(RCommon.string.common_cancel),
                        onClick = onDismiss,
                    )

                    PolkadotTextButton(
                        text = stringResource(RCommon.string.debug_menu_runtime_restart_confirm),
                        onClick = onConfirm,
                    )
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun UrlDialog(
    title: String,
    hint: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var url by remember { mutableStateOf("") }

    BasicAlertDialog(onDismissRequest = onDismiss) {
        PolkadotSurface(
            color = PolkadotTheme.colors.bg.surface.container,
            shape = RoundedCornerShape(28.dp),
            shadowElevation = 5.dp
        ) {
            Column(
                modifier = Modifier.padding(PolkadotTheme.spacings.large),
            ) {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.headline.small,
                    color = PolkadotTheme.colors.fg.primary
                )

                VerticalSpacer { mediumIncreased }

                NovaTextField(
                    modifier = Modifier.fillMaxWidth(),
                    value = url,
                    onValueChange = { url = it },
                    placeholder = {
                        NovaText(
                            text = hint,
                            style = PolkadotTheme.typography.body.large,
                            color = PolkadotTheme.colors.fg.tertiary
                        )
                    }
                )

                VerticalSpacer { large }

                PolkadotTextButton(
                    modifier = Modifier.align(Alignment.End),
                    text = stringResource(RCommon.string.debug_spa_browser_open),
                    onClick = { onConfirm(url) },
                    enabled = url.isNotBlank()
                )
            }
        }
    }
}

@Composable
private fun DebugMenuItem(
    title: String,
    enabled: Boolean,
    onClick: () -> Unit
) {
    PolkadotTextButton(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.large),
        text = title,
        onClick = onClick,
        enabled = enabled
    )
}

@Composable
private fun DebugMenuToggleItem(
    title: String,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onCheckedChange(!checked) }
            .padding(horizontal = PolkadotTheme.spacings.large),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaText(
            modifier = Modifier.weight(1f),
            text = title,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary
        )

        NovaSwitch(
            checked = checked,
            onCheckedChange = onCheckedChange
        )
    }
}

@Preview
@Composable
private fun DebugMenuScreenPreview() {
    PolkadotTheme {
        DebugMenuScreenInternal(
            state = DebugMenuState(),
            onBackClick = {},
            onClearBackupClick = {},
            onCopyWalletAccountClick = {},
            onCopyWalletMnemonicClick = {},
            onShareLogsClick = {},
            onOpenVideoGameClick = {},
            onPocketFacePreviewClick = {},
            onProductBotsClick = {},
            onCopyCandidateAccountClick = {},
            onRandomizeAccountClick = {},
            onOpenSpaBrowserClick = {},
            onSpaBrowserUrlEntered = {},
            onSpaBrowserDialogDismissed = {},
            onOpenDevServerClick = {},
            onDevServerAddressEntered = {},
            onDevServerDialogDismissed = {},
            onClearDotNsCacheClick = {},
            onClearJWTTokenClick = {},
            onSimulateGameResultsClick = {},
            onCoinageDebugWidgetsToggled = {},
            onTruapiRuntimeToggled = {},
            onRuntimeRestartConfirmed = {},
            onRuntimeRestartCancelled = {},
        )
    }
}
