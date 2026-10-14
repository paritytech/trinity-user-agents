package io.paritytech.polkadotapp.feature_settings_impl.presentation.common

import androidx.compose.foundation.Image
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.common.icons.SettingsSelected
import io.paritytech.polkadotapp.feature_settings_impl.presentation.common.icons.SettingsUnselected

@Composable
fun SettingsSelectionItem(
    isSelected: Boolean,
    onClick: () -> Unit,
    title: String,
    description: String,
    icon: @Composable () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(
                vertical = PolkadotTheme.spacings.extraMedium,
                horizontal = PolkadotTheme.spacings.mediumIncreased
            ),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(
            modifier = Modifier.align(Alignment.Top)
        ) {
            icon()
        }

        HorizontalSpacer { small }

        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = title,
                style = PolkadotTheme.typography.body.large,
                color = PolkadotTheme.colors.fg.primary
            )
            NovaText(
                text = description,
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.secondary
            )
        }

        SelectionIndicator(
            isSelected = isSelected
        )
    }
}

@Composable
private fun SelectionIndicator(
    isSelected: Boolean
) {
    Image(
        imageVector = if (isSelected) SettingsSelected else SettingsUnselected,
        contentDescription = "settings_icon"
    )
}

@Composable
fun SettingsSelectionDivider() {
    HorizontalDivider(
        modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased),
        color = PolkadotTheme.colors.stroke.primary,
        thickness = 1.dp,
    )
}
