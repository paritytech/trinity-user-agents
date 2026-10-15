package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.dropdown.NovaDropdownMenu
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDropdown
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductToolbarItemUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom.ProductToolbarUiModel
import kotlinx.collections.immutable.ImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

private val ProductIconSize = 24.dp
private val ProductButtonSize = 48.dp

/** The buttons of the toolbar of a product room: two in the bar, the others in an overflow menu (02-DESIGN §6.6). */
@Composable
internal fun ProductToolbarButtons(
    toolbar: ProductToolbarUiModel,
    onClick: (actionId: String) -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        toolbar.inline.forEach { item ->
            ProductToolbarButton(item = item, onClick = { onClick(item.actionId) })
        }

        if (toolbar.overflow.isNotEmpty()) {
            ProductToolbarOverflow(items = toolbar.overflow, onClick = onClick)
        }
    }
}

@Composable
private fun ProductToolbarButton(
    item: ProductToolbarItemUiModel,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .widthIn(min = ProductButtonSize)
            .clickable(onClick = onClick)
            .semantics { contentDescription = item.label }
            .padding(PolkadotTheme.spacings.small),
        contentAlignment = Alignment.Center,
    ) {
        if (item.icon != null) {
            NovaAsyncImage(
                modifier = Modifier.size(ProductIconSize),
                model = item.icon,
                contentDescription = null,
            )
        } else {
            NovaText(
                text = item.label,
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun ProductToolbarOverflow(
    items: ImmutableList<ProductToolbarItemUiModel>,
    onClick: (actionId: String) -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    val moreLabel = stringResource(RCommon.string.chat_product_toolbar_more)

    Box {
        PolkadotIconButton(
            modifier = Modifier.semantics { contentDescription = moreLabel },
            icon = NovaIcons.ArrowDropdown,
            onClick = { expanded = true },
            style = PolkadotButtonStyle.ghost(),
            size = PolkadotIconButtonSize.mediumIncreased(),
            shape = PolkadotButtonShape.pill,
        )

        NovaDropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
        ) {
            Column {
                items.forEach { item ->
                    Row(
                        modifier = Modifier
                            .clickable {
                                expanded = false
                                onClick(item.actionId)
                            }
                            .padding(
                                horizontal = PolkadotTheme.spacings.mediumIncreased,
                                vertical = PolkadotTheme.spacings.medium,
                            ),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(modifier = Modifier.size(ProductIconSize)) {
                            if (item.icon != null) {
                                NovaAsyncImage(modifier = Modifier.size(ProductIconSize), model = item.icon, contentDescription = null)
                            }
                        }

                        HorizontalSpacer { smallIncreased }

                        NovaText(
                            text = item.label,
                            style = PolkadotTheme.typography.body.large,
                            color = PolkadotTheme.colors.fg.primary,
                        )
                    }
                }
            }
        }
    }
}
