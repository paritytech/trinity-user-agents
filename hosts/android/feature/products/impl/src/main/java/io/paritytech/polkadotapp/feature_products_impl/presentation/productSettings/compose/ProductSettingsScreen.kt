package io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.onLoaded
import io.paritytech.polkadotapp.common.presentation.loading.onLoading
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.compose.ProductSettingsItem
import io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.ProductSettingsViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.compose.components.ProductSettingsHeader
import io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.compose.components.ProductSettingsSection
import io.paritytech.polkadotapp.feature_products_impl.presentation.productSettings.models.ProductSettingsUiModel
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ProductSettingsScreen(viewModel: ProductSettingsViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()

    ProductSettingsScreenInternal(
        state = state,
        onBack = viewModel::onBack,
        onPermissionsClick = viewModel::onPermissionsClick
    )
}

@Composable
private fun ProductSettingsScreenInternal(
    state: LoadingState<ProductSettingsUiModel>,
    onBack: () -> Unit,
    onPermissionsClick: () -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
        ) {
            state
                .onLoaded { uiModel ->
                    PolkadotTopBar(
                        title = uiModel.name,
                        navigationAction = rememberTopBarAction(action = onBack),
                        titleAlignment = TopBarTitleAlignment.Center,
                    )

                    Column(
                        modifier = Modifier
                            .fillMaxSize()
                            .verticalScroll(rememberScrollState())
                    ) {
                        VerticalSpacer { mediumIncreased }

                        ProductSettingsHeader(uiModel)

                        if (uiModel.hasPermissions) {
                            VerticalSpacer { large }

                            ProductSettingsSection(
                                title = stringResource(RCommon.string.product_settings_section_privacy).uppercase()
                            )

                            ProductSettingsItem(
                                title = stringResource(RCommon.string.product_settings_permissions),
                                onClick = onPermissionsClick
                            )
                        }
                    }
                }
                .onLoading {
                    LoadingScreenState()
                }
        }
    }
}

@Preview
@Composable
private fun ProductSettingsScreenPreview() {
    PolkadotTheme {
        ProductSettingsScreenInternal(
            state = LoadingState.Loaded(
                ProductSettingsUiModel(
                    name = "Web3 Summit",
                    hasPermissions = true,
                    iconUrl = null,
                )
            ),
            onBack = {},
            onPermissionsClick = {}
        )
    }
}
