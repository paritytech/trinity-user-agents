package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun Footer(
    onConfirm: (() -> Unit)?
) {
    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(
            topStart = PolkadotTheme.radii.extraLarge,
            topEnd = PolkadotTheme.radii.extraLarge
        ),
        color = PolkadotTheme.colors.bg.surface.container
    ) {
        Column(
            modifier = Modifier
                .padding(
                    top = PolkadotTheme.spacings.large,
                    bottom = PolkadotTheme.spacings.mediumIncreased
                )
                .navigationBarsPadding()
        ) {
            Column(
                modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.large)
            ) {
                NovaText(
                    text = stringResource(RCommon.string.settings_theme_claim_title),
                    style = PolkadotTheme.typography.headline.large,
                    color = PolkadotTheme.colors.fg.primary
                )

                VerticalSpacer { small }

                NovaText(
                    text = stringResource(RCommon.string.settings_theme_claim_subtitle),
                    style = PolkadotTheme.typography.paragraph.large,
                    color = PolkadotTheme.colors.fg.secondary
                )
            }

            if (onConfirm != null) {
                VerticalSpacer { extraLarge }

                PolkadotTextButton(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
                    text = stringResource(RCommon.string.common_continue),
                    shape = PolkadotButtonShape.pill,
                    onClick = onConfirm
                )
            }
        }
    }
}
