package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.domain.product.PendingProductUpdate
import io.paritytech.polkadotapp.common.R as RCommon

private val ProductIconSize = 24.dp

private const val MESSAGE_MAX_LINES = 2

@Composable
fun ProductUpdateNotice(
    modifier: Modifier = Modifier,
    pendingUpdate: PendingProductUpdate?,
    onUpdateClick: () -> Unit,
    onDismissClick: () -> Unit,
) {
    var shownUpdate by remember { mutableStateOf(pendingUpdate) }
    if (pendingUpdate != null) shownUpdate = pendingUpdate

    TopNotice(modifier = modifier, isVisible = pendingUpdate != null) {
        shownUpdate?.let { update ->
            ProductUpdateNoticeContent(
                pendingUpdate = update,
                onUpdateClick = onUpdateClick,
                onDismissClick = onDismissClick,
            )
        }
    }
}

@Composable
private fun RowScope.ProductUpdateNoticeContent(
    pendingUpdate: PendingProductUpdate,
    onUpdateClick: () -> Unit,
    onDismissClick: () -> Unit,
) {
    val closeDescription = stringResource(RCommon.string.common_close)

    NovaAsyncImage(
        modifier = Modifier
            .size(ProductIconSize)
            .clip(PolkadotTheme.shapes.small),
        model = pendingUpdate.iconUri,
        contentScale = ContentScale.Crop,
    )

    NovaText(
        modifier = Modifier.weight(1f),
        text = stringResource(RCommon.string.product_update_notice_message, pendingUpdate.productName),
        style = PolkadotTheme.typography.body.small,
        color = PolkadotTheme.colors.fg.primary,
        maxLines = MESSAGE_MAX_LINES,
    )

    PolkadotTextButton(
        text = stringResource(RCommon.string.product_update_notice_action),
        style = PolkadotButtonStyle.primary(),
        size = PolkadotButtonSize.medium(),
        shape = PolkadotButtonShape.pill,
        onClick = onUpdateClick,
    )

    PolkadotIconButton(
        modifier = Modifier.semantics { contentDescription = closeDescription },
        icon = NovaIcons.Close,
        onClick = onDismissClick,
        style = PolkadotButtonStyle.ghost(),
        size = PolkadotIconButtonSize.small(),
        shape = PolkadotTheme.shapes.full,
    )
}

@Preview
@Composable
private fun ProductUpdateNoticePreview() {
    ProductUpdateNotice(
        modifier = Modifier.fillMaxSize(),
        pendingUpdate = PendingProductUpdate(productName = "KartKart", iconUri = "", dotNsName = "app.kartkart.dot"),
        onUpdateClick = {},
        onDismissClick = {},
    )
}
