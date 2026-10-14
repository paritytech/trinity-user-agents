package io.paritytech.polkadotapp.app.root.presentation.main.compose

import androidx.compose.animation.Crossfade
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.app.root.presentation.main.MainViewModel
import io.paritytech.polkadotapp.common.presentation.tabbar.ForceShowTabBar
import io.paritytech.polkadotapp.common.presentation.tabs.BottomTab
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ChatFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.GlobeAltFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.SettingsFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.WalletFilled
import io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.ChatListScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.exploreProducts.compose.ExploreProductsScreen
import io.paritytech.polkadotapp.feature_settings_impl.presentation.main.SettingsScreen
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.PocketScreen
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun MainScreen(viewModel: MainViewModel) {
    // On Main the bar is always available and can never be hidden.
    ForceShowTabBar()

    val currentTab by viewModel.currentTab.collectAsStateWithLifecycle()

    MainScreenInternal(currentTab = currentTab)
}

@Composable
private fun MainScreenInternal(currentTab: BottomTab) {
    // The nav-bar inset (LocalAppNavigationBarInsets) is provided by MainFragment from the bar height.
    Box(modifier = Modifier.fillMaxSize()) {
        Crossfade(
            modifier = Modifier.fillMaxSize(),
            targetState = currentTab,
        ) {
            when (it) {
                BottomTab.CHATS -> ChatListScreen()
                BottomTab.WALLET -> PocketScreen()
                BottomTab.EXPLORE -> ExploreProductsScreen()
                BottomTab.SETTINGS -> SettingsScreen()
            }
        }
    }
}

fun BottomTab.icon() = when (this) {
    BottomTab.CHATS -> NovaIcons.ChatFilled
    BottomTab.WALLET -> NovaIcons.WalletFilled
    BottomTab.EXPLORE -> NovaIcons.GlobeAltFilled
    BottomTab.SETTINGS -> NovaIcons.SettingsFilled
}

@Composable
fun BottomTab.title() = stringResource(
    when (this) {
        BottomTab.CHATS -> RCommon.string.bottom_nav_menu_chats
        BottomTab.WALLET -> RCommon.string.bottom_nav_menu_pocket
        BottomTab.EXPLORE -> RCommon.string.bottom_nav_menu_explore
        BottomTab.SETTINGS -> RCommon.string.bottom_nav_menu_settings
    }
)
