package io.paritytech.polkadotapp.feature_settings_impl.presentation.legalAndSupport.compose

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.menu.PolkadotMenuList
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.common.SettingsMenuItem
import io.paritytech.polkadotapp.feature_settings_impl.presentation.legalAndSupport.LegalAndSupportContract
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun LegalAndSupportScreen(contract: LegalAndSupportContract) {
    LegalAndSupportScreenInternal(
        onBackClick = contract::onBackClick,
        onPrivacyPolicyClick = contract::onPrivacyPolicyClick,
        onTermsOfUseClick = contract::onTermsOfUseClick,
        onContactUsClick = contract::onContactUsClick
    )
}

@Composable
private fun LegalAndSupportScreenInternal(
    onBackClick: () -> Unit,
    onPrivacyPolicyClick: () -> Unit,
    onTermsOfUseClick: () -> Unit,
    onContactUsClick: () -> Unit
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .safeDrawingPadding()
        ) {
            PolkadotTopBar(
                title = stringResource(RCommon.string.settings_legal_and_support),
                titleAlignment = TopBarTitleAlignment.Center,
                navigationAction = rememberTopBarAction(onBackClick)
            )

            VerticalSpacer { large }

            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = PolkadotTheme.spacings.large)
            ) {
                PolkadotMenuList(
                    headerText = stringResource(RCommon.string.settings_section_legal)
                ) {
                    SettingsMenuItem(
                        title = stringResource(RCommon.string.settings_privacy_policy),
                        onClick = onPrivacyPolicyClick
                    )
                    SettingsMenuItem(
                        title = stringResource(RCommon.string.settings_terms_of_use),
                        onClick = onTermsOfUseClick
                    )
                }

                VerticalSpacer { large }

                PolkadotMenuList(
                    headerText = stringResource(RCommon.string.settings_section_support)
                ) {
                    SettingsMenuItem(
                        title = stringResource(RCommon.string.settings_contact_us),
                        onClick = onContactUsClick
                    )
                }
            }
        }
    }
}

@Preview
@Composable
private fun LegalAndSupportScreenPreview() {
    PolkadotTheme {
        LegalAndSupportScreenInternal(
            onBackClick = {},
            onPrivacyPolicyClick = {},
            onTermsOfUseClick = {},
            onContactUsClick = {}
        )
    }
}
