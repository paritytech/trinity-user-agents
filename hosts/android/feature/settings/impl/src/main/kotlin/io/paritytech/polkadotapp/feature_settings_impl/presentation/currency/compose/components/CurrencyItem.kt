package io.paritytech.polkadotapp.feature_settings_impl.presentation.currency.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Dollar
import io.paritytech.polkadotapp.design.components.icon.vectors.Euro
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_settings_impl.presentation.common.SettingsSelectionItem

@Composable
fun CurrencyItem(
    title: String,
    subtitle: String,
    currencyIcon: ImageVector,
    isSelected: Boolean,
    onClick: () -> Unit
) {
    SettingsSelectionItem(
        isSelected = isSelected,
        onClick = onClick,
        title = title,
        description = subtitle,
        icon = {
            NovaIcon(
                modifier = Modifier.size(20.dp),
                imageVector = currencyIcon,
                tint = Color.Unspecified
            )
        }
    )
}

@Preview
@Composable
private fun CurrencyItemPreview() {
    PolkadotTheme {
        PolkadotSurface {
            Column {
                CurrencyItem(
                    title = "US Dollar",
                    subtitle = "USD",
                    currencyIcon = NovaIcons.Dollar,
                    isSelected = true,
                    onClick = {}
                )
                CurrencyItem(
                    title = "Euro",
                    subtitle = "EUR",
                    currencyIcon = NovaIcons.Euro,
                    isSelected = false,
                    onClick = {}
                )
            }
        }
    }
}
