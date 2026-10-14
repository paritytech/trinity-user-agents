package io.paritytech.polkadotapp.app.root.presentation.root.compose.chatoverlay

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import androidx.navigation.NavDestination
import androidx.navigation.fragment.FragmentNavigator
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

@Composable
fun ChatExtensionOverlayHost(
    navController: NavController,
    overlays: Flow<List<ChatOverlay>>,
    isOnboarded: Flow<Boolean>,
    bottomNavHeight: Flow<Dp>,
) {
    val onboarded by isOnboarded.collectAsStateWithLifecycle(initialValue = false)
    val overlayList by overlays.collectAsStateWithLifecycle(initialValue = emptyList())
    val navHeight by bottomNavHeight.collectAsStateWithLifecycle(initialValue = 0.dp)

    val currentDestination by remember(navController) {
        navController.currentBackStackEntryFlow.map { it.destination }
    }.collectAsStateWithLifecycle(initialValue = null)

    val currentFragmentClass = currentDestination.fragmentClassName()

    PolkadotTheme {
        val overlayBottomPadding = navHeight + PolkadotTheme.spacings.mediumIncreased
        val systemInsetModifier = if (navHeight > 0.dp) Modifier else Modifier.navigationBarsPadding()

        Column(
            modifier = Modifier
                .fillMaxSize()
                .then(systemInsetModifier),
            verticalArrangement = Arrangement.Bottom,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            overlayList.forEach { overlay ->
                val isOnOwnedScreen = currentFragmentClass != null &&
                    currentFragmentClass in overlay.ownedFragmentClasses

                AnimatedVisibility(
                    visible = onboarded && !isOnOwnedScreen,
                    enter = slideInVertically(initialOffsetY = { it }) + fadeIn(),
                    exit = slideOutVertically(targetOffsetY = { it }) + fadeOut(),
                ) {
                    Column(
                        modifier = Modifier.padding(
                            start = PolkadotTheme.spacings.mediumIncreased,
                            end = PolkadotTheme.spacings.mediumIncreased,
                            top = PolkadotTheme.spacings.small,
                            bottom = overlayBottomPadding,
                        ),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        overlay.renderer.DrawOverlay()
                    }
                }
            }
        }
    }
}

private fun NavDestination?.fragmentClassName(): String? =
    (this as? FragmentNavigator.Destination)?.className
