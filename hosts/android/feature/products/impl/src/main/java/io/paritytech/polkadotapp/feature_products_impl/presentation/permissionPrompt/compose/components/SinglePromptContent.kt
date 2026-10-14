package io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.components

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.icon
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.subtitle
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.title

@Composable
internal fun SinglePromptContent(
    productId: String,
    permission: ProductPermission,
) {
    NovaIcon(
        modifier = Modifier.size(72.dp),
        imageVector = permission.icon,
        tint = PolkadotTheme.colors.fg.primary,
    )

    VerticalSpacer { mediumIncreased }

    NovaText(
        modifier = Modifier.fillMaxWidth(),
        text = permission.title(productId),
        style = PolkadotTheme.typography.headline.small,
        textAlign = TextAlign.Center,
    )

    VerticalSpacer { mediumIncreased }

    NovaText(
        modifier = Modifier.fillMaxWidth(),
        text = permission.subtitle(),
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.secondary,
        textAlign = TextAlign.Center,
    )
}

@Preview
@Composable
private fun SinglePromptContentPreview() {
    PolkadotTheme {
        Column {
            SinglePromptContent(
                productId = "alice.dot",
                permission = ProductPermission.BalanceAccess,
            )
        }
    }
}
