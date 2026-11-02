package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.compose

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
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
import io.paritytech.polkadotapp.design.components.dropdown.NovaDropdownMenu
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ChatFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.PolkadotMark
import io.paritytech.polkadotapp.design.components.icon.vectors.Refreshing
import io.paritytech.polkadotapp.design.components.icon.vectors.Share
import io.paritytech.polkadotapp.design.components.menu.NovaMenuOption
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductChat
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductMenu
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductStatusBarUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar.ProductStatusBarViewModel
import io.paritytech.polkadotapp.common.R as RCommon

private val LogoSize = 16.dp

@Composable
fun ProductStatusBarTitle() {
    val viewModel = hiltViewModel<ProductStatusBarViewModel>()
    val state by viewModel.state.collectAsStateWithLifecycle()

    ProductStatusBarTitleInternal(
        state = state,
        onProceedInChatClick = viewModel::onProceedInChatClick,
        onReloadClick = viewModel::onReloadClick,
        onShareClick = viewModel::onShareClick,
        onCloseAppClick = viewModel::onCloseAppClick,
    )
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

        NovaDropdownMenu(
            expanded = menuExpanded,
            onDismissRequest = { menuExpanded = false },
        ) {
            Column {
                val menu = state.menu

                if (menu is ProductMenu.Product && menu.chat == ProductChat.Served) {
                    ProductMenuOption(
                        text = stringResource(RCommon.string.spa_browser_menu_proceed_in_chat),
                        icon = NovaIcons.ChatFilled,
                        onClick = { select(onProceedInChatClick) },
                    )
                }

                ProductMenuOption(
                    text = stringResource(RCommon.string.spa_browser_menu_reload),
                    icon = NovaIcons.Refreshing,
                    onClick = { select(onReloadClick) },
                )

                if (menu is ProductMenu.Product) {
                    ProductMenuOption(
                        text = stringResource(RCommon.string.spa_browser_menu_share),
                        icon = NovaIcons.Share,
                        onClick = { select(onShareClick) },
                    )
                }

                HorizontalDivider(color = PolkadotTheme.colors.stroke.primary)

                ProductMenuOption(
                    text = stringResource(RCommon.string.spa_browser_menu_close_app),
                    icon = NovaIcons.Close,
                    onClick = { select(onCloseAppClick) },
                )
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
    NovaMenuOption(
        text = text,
        icon = icon,
        color = PolkadotTheme.colors.fg.primary,
        onClick = onClick,
    )
}

@Preview(backgroundColor = 0xFF000000, showBackground = true)
@Composable
private fun ProductStatusBarTitlePreview() {
    PolkadotTheme {
        ProductStatusBarTitleInternal(
            state = ProductStatusBarUiState(title = "Coin Flip", menu = ProductMenu.Product(ProductId.fromStoredValue("coinflip.paseo"), ProductChat.Served)),
            onProceedInChatClick = {},
            onReloadClick = {},
            onShareClick = {},
            onCloseAppClick = {},
        )
    }
}
