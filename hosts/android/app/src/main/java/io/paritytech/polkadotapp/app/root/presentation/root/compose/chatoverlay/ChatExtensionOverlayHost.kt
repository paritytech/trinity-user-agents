package io.paritytech.polkadotapp.app.root.presentation.root.compose.chatoverlay

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavController
import androidx.navigation.NavDestination
import androidx.navigation.fragment.FragmentNavigator
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlay
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatOverlayInsets
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
            // What each overlay draws, by its index; a renderer that draws nothing (no pill right now) counts as 0.
            val drawn = remember { mutableStateMapOf<Int, Int>() }
            val density = LocalDensity.current
            val paddingPx = with(density) { (PolkadotTheme.spacings.small + overlayBottomPadding).roundToPx() }
            LaunchedEffect(drawn.toMap(), paddingPx) {
                // The chat lifts its input slot by this, above the navigation bar the host already pads for.
                ChatOverlayInsets.reportBottom(drawn.values.filter { it > 0 }.sumOf { it + paddingPx })
            }
            DisposableEffect(Unit) { onDispose { ChatOverlayInsets.reportBottom(0) } }

            overlayList.forEachIndexed { index, overlay ->
                val isOnOwnedScreen = currentFragmentClass != null &&
                    currentFragmentClass in overlay.ownedFragmentClasses
                val visible = onboarded && !isOnOwnedScreen
                LaunchedEffect(visible) { if (!visible) drawn.remove(index) }

                AnimatedVisibility(
                    visible = visible,
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
                        Box(modifier = Modifier.onSizeChanged { drawn[index] = it.height }) {
                            overlay.renderer.DrawOverlay()
                        }
                    }
                }
            }
        }
    }
}

private fun NavDestination?.fragmentClassName(): String? =
    (this as? FragmentNavigator.Destination)?.className
