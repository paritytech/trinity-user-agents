package io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.fastForEach
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission.RemotePermission
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.icon
import io.paritytech.polkadotapp.feature_products_impl.presentation.permissionPrompt.compose.shortLabel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun BatchedPromptContent(
    productId: String,
    permissions: ImmutableList<RemotePermission>,
) {
    NovaText(
        modifier = Modifier.fillMaxWidth(),
        text = stringResource(RCommon.string.product_permission_batched_title, productId),
        style = PolkadotTheme.typography.headline.small,
        textAlign = TextAlign.Center,
    )

    VerticalSpacer { mediumIncreased }

    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
    ) {
        permissions.fastForEach { permission ->
            PermissionRow(permission)
        }
    }

    VerticalSpacer { mediumIncreased }

    NovaText(
        modifier = Modifier.fillMaxWidth(),
        text = stringResource(RCommon.string.product_permission_subtitle),
        style = PolkadotTheme.typography.body.large,
        color = PolkadotTheme.colors.fg.secondary,
        textAlign = TextAlign.Center,
    )
}

@Composable
private fun PermissionRow(permission: RemotePermission) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
    ) {
        NovaIcon(
            modifier = Modifier.size(24.dp),
            imageVector = permission.icon,
            tint = PolkadotTheme.colors.fg.primary,
        )

        NovaText(
            text = permission.shortLabel(),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

@Preview
@Composable
private fun BatchedPromptContentPreview() {
    PolkadotTheme {
        Column {
            BatchedPromptContent(
                productId = "alice.dot",
                permissions = persistentListOf(
                    RemotePermission.NetworkAccess(domain = "example.com"),
                    RemotePermission.WebRtcAccess,
                    RemotePermission.ChainSubmitAccess,
                ),
            )
        }
    }
}
