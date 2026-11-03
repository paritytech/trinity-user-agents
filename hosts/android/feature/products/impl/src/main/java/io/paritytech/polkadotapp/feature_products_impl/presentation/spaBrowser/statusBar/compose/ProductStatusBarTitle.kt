package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.HorizontalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.hilt.lifecycle.viewmodel.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.docklet.DockletGlassTheme
import io.paritytech.polkadotapp.design.components.docklet.LocalDockletPalette
import io.paritytech.polkadotapp.design.components.dropdown.NovaDropdownMenu
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.PolkadotMark
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductChat
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductMenu
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductStatusBarUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductStatusBarViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.components.icons.ChatBubbleOvalLeft
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.components.icons.ExternalLink
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.components.icons.RotateCw
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose.components.icons.XMark
import io.paritytech.polkadotapp.common.R as RCommon

private val LogoSize = 16.dp
private val MenuWidth = 238.dp
private val MenuIconSize = 20.dp
private val MenuItemHeight = 40.dp

@Composable
fun ProductStatusBarTitle() {
    val viewModel = hiltViewModel<ProductStatusBarViewModel>()
    val state by viewModel.state.collectAsStateWithLifecycle()

    state?.let { shown ->
        ProductStatusBarTitleInternal(
            state = shown,
            onProceedInChatClick = viewModel::onProceedInChatClick,
            onReloadClick = viewModel::onReloadClick,
            onShareClick = viewModel::onShareClick,
            onCloseAppClick = viewModel::onCloseAppClick,
        )
    }
}

@Composable
private fun ProductStatusBarTitleInternal(
    state: ProductStatusBarUiState,
    onProceedInChatClick: () -> Unit,
    onReloadClick: () -> Unit,
    onShareClick: () -> Unit,
    onCloseAppClick: () -> Unit,
) {
    var menuExpanded by remember { mutableStateOf(false) }

    fun select(action: () -> Unit) {
        menuExpanded = false
        action()
    }

    Box {
        Row(
            modifier = Modifier.clickable(role = Role.Button, onClick = { menuExpanded = true }),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NovaIcon(
                modifier = Modifier.size(LogoSize),
                imageVector = NovaIcons.PolkadotMark,
                tint = PolkadotTheme.colors.fg.primary,
            )

            HorizontalSpacer { smallIncreased }

            NovaText(
                text = state.title,
                style = PolkadotTheme.typography.title.small,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }

        DockletGlassTheme {
            val glass = LocalDockletPalette.current

            NovaDropdownMenu(
                modifier = Modifier.width(MenuWidth),
                expanded = menuExpanded,
                onDismissRequest = { menuExpanded = false },
                shape = PolkadotTheme.shapes.extraLarge,
                color = glass.glassFill,
                border = BorderStroke(PolkadotTheme.borders.default, glass.glassStroke),
            ) {
                Column(modifier = Modifier.padding(PolkadotTheme.spacings.small)) {
                    val menu = state.menu

                    if (menu is ProductMenu.Product && menu.chat == ProductChat.Served) {
                        ProductMenuOption(
                            text = stringResource(RCommon.string.spa_browser_menu_proceed_in_chat),
                            icon = ChatBubbleOvalLeft,
                            onClick = { select(onProceedInChatClick) },
                        )
                    }

                    ProductMenuOption(
                        text = stringResource(RCommon.string.spa_browser_menu_reload),
                        icon = RotateCw,
                        onClick = { select(onReloadClick) },
                    )

                    if (menu is ProductMenu.Product) {
                        ProductMenuOption(
                            text = stringResource(RCommon.string.spa_browser_menu_share),
                            icon = ExternalLink,
                            onClick = { select(onShareClick) },
                        )
                    }

                    HorizontalDivider(
                        modifier = Modifier.padding(
                            horizontal = PolkadotTheme.spacings.small,
                            vertical = PolkadotTheme.spacings.smallIncreased,
                        ),
                        color = PolkadotTheme.colors.stroke.primary,
                    )

                    ProductMenuOption(
                        text = stringResource(RCommon.string.spa_browser_menu_close_app),
                        icon = XMark,
                        onClick = { select(onCloseAppClick) },
                    )
                }
            }
        }
    }
}

@Composable
private fun ProductMenuOption(
    text: String,
    icon: ImageVector,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = MenuItemHeight)
            .clickable(onClick = onClick)
            .padding(
                start = PolkadotTheme.spacings.extraMedium,
                end = PolkadotTheme.spacings.small,
                top = PolkadotTheme.spacings.small,
                bottom = PolkadotTheme.spacings.small,
            ),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
    ) {
        NovaIcon(
            modifier = Modifier.size(MenuIconSize),
            imageVector = icon,
            tint = PolkadotTheme.colors.fg.primary,
        )

        NovaText(
            text = text,
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun ProductStatusBarTitlePreview() {
    PolkadotTheme {
        ProductStatusBarTitleInternal(
            state = ProductStatusBarUiState(title = "Coin Flip", menu = ProductMenu.WebPage),
            onProceedInChatClick = {},
            onReloadClick = {},
            onShareClick = {},
            onCloseAppClick = {},
        )
    }
}
