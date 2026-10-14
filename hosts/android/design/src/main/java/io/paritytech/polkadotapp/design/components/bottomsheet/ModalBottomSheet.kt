@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.design.components.bottomsheet

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ModalBottomSheetProperties
import androidx.compose.material3.SheetState
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogWindowProvider
import androidx.compose.ui.window.SecureFlagPolicy
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.CloseKeyboardOnContentAppearance

@Composable
fun NovaModalBottomSheet(
    isVisible: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    sheetState: SheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    scrimColor: Color = Color.Black.copy(alpha = 0.34f),
    shouldDismissOnBackPress: Boolean = true,
    shouldDismissOnClickOutside: Boolean = true,
    isAppearanceLightStatusBars: Boolean = false,
    isAppearanceLightNavigationBars: Boolean = false,
    securePolicy: SecureFlagPolicy = SecureFlagPolicy.Inherit,
    content: @Composable BoxScope.() -> Unit,
) {
    val properties = remember(
        shouldDismissOnBackPress,
        shouldDismissOnClickOutside,
        isAppearanceLightStatusBars,
        isAppearanceLightNavigationBars,
        securePolicy
    ) {
        ModalBottomSheetProperties(
            isAppearanceLightStatusBars = isAppearanceLightStatusBars,
            isAppearanceLightNavigationBars = isAppearanceLightNavigationBars,
            securePolicy = securePolicy,
            shouldDismissOnBackPress = shouldDismissOnBackPress,
            shouldDismissOnClickOutside = shouldDismissOnClickOutside
        )
    }

    CloseKeyboardOnContentAppearance(isVisible = isVisible) { effectivelyVisible ->
        LaunchedEffect(effectivelyVisible) {
            if (effectivelyVisible) {
                sheetState.show()
            }
        }

        LaunchedEffect(isVisible) {
            if (!isVisible) {
                sheetState.hide()
                onDismissRequest()
            }
        }

        if (sheetState.isVisible || effectivelyVisible) {
            ModalBottomSheet(
                onDismissRequest = onDismissRequest,
                modifier = modifier,
                sheetState = sheetState,
                scrimColor = scrimColor,
                containerColor = Color.Transparent,
                dragHandle = {},
                properties = properties
            ) {
                DisableSystemBarContrastEnforcement()

                NovaBottomSheetSurface {
                    content()
                }
            }
        }
    }
}

@Composable
private fun DisableSystemBarContrastEnforcement() {
    val dialogWindow = (LocalView.current.parent as? DialogWindowProvider)?.window
    if (dialogWindow != null) {
        SideEffect {
            @Suppress("DEPRECATION")
            dialogWindow.isStatusBarContrastEnforced = false
            @Suppress("DEPRECATION")
            dialogWindow.isNavigationBarContrastEnforced = false
        }
    }
}

@Composable
fun NovaBottomSheetSurface(
    content: @Composable BoxScope.() -> Unit
) {
    PolkadotSurface(
        modifier = Modifier
            .fillMaxWidth()
            .padding(PolkadotTheme.spacings.small)
            .navigationBarsPadding(),
        shape = PolkadotTheme.shapes.extraLarge,
        color = PolkadotTheme.colors.bg.surface.container,
        content = content
    )
}

@Composable
fun NovaBottomSheetDragHandler() {
    Box(
        modifier = Modifier
            .size(
                width = 48.dp,
                height = 5.dp
            )
            .background(
                color = PolkadotTheme.colors.fg.tertiary,
                shape = PolkadotTheme.shapes.tiny
            )
    )
}
