package io.paritytech.polkadotapp.feature_settings_impl.presentation.main

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.BlockOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.FileOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.GridOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.Language
import io.paritytech.polkadotapp.design.components.icon.vectors.LaptopOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.NotificationsBellOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.PaletteOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.Settings
import io.paritytech.polkadotapp.design.components.menu.PolkadotMenuList
import io.paritytech.polkadotapp.design.components.menu.PolkadotMenuListCustomItem
import io.paritytech.polkadotapp.design.components.navigationbar.LocalAppNavigationBarInsets
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleSize
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingStrategyType
import io.paritytech.polkadotapp.feature_settings_impl.presentation.common.BackupSettingsMenuItem
import io.paritytech.polkadotapp.feature_settings_impl.presentation.common.SettingsMenuItem
import io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.AppDeviceInfoSection
import io.paritytech.polkadotapp.feature_settings_impl.presentation.main.components.privacyMode.PaymentPrivacyModeSelector
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun SettingsScreen() {
    val viewModel = hiltViewModel<SettingsViewModel>()
    val state by viewModel.state.collectAsStateWithLifecycle()

    SettingsScreenInternal(
        state = state,
        onNotificationsClick = viewModel::onNotificationsClick,
        onLanguageClick = viewModel::onLanguageClick,
        onThemeClick = viewModel::onThemeClick,
        onPrivacyModeSelected = viewModel::onPrivacyModeSelected,
        onBackupClick = viewModel::onBackupClick,
        onProductsClick = viewModel::onProductsClick,
        onBlockedUsersClick = viewModel::onBlockedUsersClick,
        onConnectedDevicesClick = viewModel::onLinkedDevicesClick,
        onLegalAndSupportClick = viewModel::onLegalAndSupportClick,
        onDebugMenuClick = viewModel::onDebugMenuClick
    )
}

@Composable
private fun SettingsScreenInternal(
    state: SettingsUiState,
    onNotificationsClick: () -> Unit,
    onLanguageClick: () -> Unit,
    onThemeClick: () -> Unit,
    onPrivacyModeSelected: (RecyclingStrategyType) -> Unit,
    onBackupClick: () -> Unit,
    onProductsClick: () -> Unit,
    onBlockedUsersClick: () -> Unit,
    onConnectedDevicesClick: () -> Unit,
    onLegalAndSupportClick: () -> Unit,
    onDebugMenuClick: () -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .statusBarsPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.settings_toolbar_title),
                titleSize = TopBarTitleSize.Large,
            )

            VerticalSpacer { large }

            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .windowInsetsPadding(LocalAppNavigationBarInsets.current)
                    .padding(horizontal = PolkadotTheme.spacings.large)
            ) {
                PolkadotMenuList(
                    headerText = stringResource(RCommon.string.settings_section_general)
                ) {
                    SettingsMenuItem(
                        icon = NovaIcons.PaletteOutlined,
                        title = stringResource(RCommon.string.settings_theme),
                        label = state.selectedTheme.id,
                        onClick = onThemeClick
                    )
                    SettingsMenuItem(
                        icon = NovaIcons.NotificationsBellOutlined,
                        title = stringResource(RCommon.string.settings_notifications),
                        onClick = onNotificationsClick
                    )
                    if (state.isLanguageSettingsAvailable) {
                        SettingsMenuItem(
                            icon = NovaIcons.Language,
                            title = stringResource(RCommon.string.settings_language),
                            onClick = onLanguageClick
                        )
                    }
                }

                VerticalSpacer { large }

                PolkadotMenuList(
                    headerText = stringResource(RCommon.string.settings_section_security)
                ) {
                    state.privacyMode?.let { privacyMode ->
                        PolkadotMenuListCustomItem { shape ->
                            PaymentPrivacyModeSelector(
                                modifier = Modifier.fillMaxWidth(),
                                shape = shape,
                                selectedMode = privacyMode,
                                onModeSelected = onPrivacyModeSelected
                            )
                        }
                    }
                    BackupSettingsMenuItem(
                        onClick = onBackupClick,
                        isBackupMissing = state.isBackupMissing
                    )
                    if (state.productSettingsEnabled) {
                        SettingsMenuItem(
                            icon = NovaIcons.GridOutlined,
                            title = stringResource(RCommon.string.settings_products),
                            onClick = onProductsClick
                        )
                    }
                    SettingsMenuItem(
                        icon = NovaIcons.BlockOutlined,
                        title = stringResource(RCommon.string.settings_blocked_contacts),
                        onClick = onBlockedUsersClick
                    )
                    if (state.linkedDevicesEnabled) {
                        SettingsMenuItem(
                            icon = NovaIcons.LaptopOutlined,
                            title = stringResource(RCommon.string.settings_connected_devices),
                            onClick = onConnectedDevicesClick
                        )
                    }
                }

                VerticalSpacer { large }

                PolkadotMenuList(
                    headerText = stringResource(RCommon.string.settings_legal_and_support)
                ) {
                    SettingsMenuItem(
                        icon = NovaIcons.FileOutlined,
                        title = stringResource(RCommon.string.settings_legal_and_support),
                        onClick = onLegalAndSupportClick
                    )
                }

                VerticalSpacer { large }

                if (state.debugMenuEnabled) {
                    PolkadotMenuList(
                        headerText = stringResource(RCommon.string.settings_section_debug)
                    ) {
                        SettingsMenuItem(
                            icon = NovaIcons.Settings,
                            title = stringResource(RCommon.string.settings_debug_menu),
                            onClick = onDebugMenuClick
                        )
                    }

                    VerticalSpacer { large }
                }

                AppDeviceInfoSection(state.debugMenuEnabled)
            }
        }
    }
}

@Preview(device = "spec:width=1080px,height=3000px,dpi=440")
@Composable
private fun SettingsScreenPreview() = CompositionLocalProvider(
    LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
) {
    PolkadotTheme {
        SettingsScreenInternal(
            state = SettingsUiState(
                debugMenuEnabled = true,
                linkedDevicesEnabled = true,
                productSettingsEnabled = true,
                selectedTheme = PolkadotAppTheme.DEFAULT,
                privacyMode = RecyclingStrategyType.BALANCED,
                isLanguageSettingsAvailable = true,
                isBackupMissing = false,
                hasBlockedUsers = false
            ),
            onNotificationsClick = {},
            onLanguageClick = {},
            onThemeClick = {},
            onPrivacyModeSelected = {},
            onBackupClick = {},
            onProductsClick = {},
            onBlockedUsersClick = {},
            onConnectedDevicesClick = {},
            onLegalAndSupportClick = {},
            onDebugMenuClick = {}
        )
    }
}
