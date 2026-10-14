@file:OptIn(ExperimentalLayoutApi::class)

package io.paritytech.polkadotapp.design.utils

import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalSoftwareKeyboardController

@Composable
fun CloseKeyboardOnContentAppearance(
    isVisible: Boolean,
    content: @Composable (effectivelyVisible: Boolean) -> Unit
) {
    val keyboardController = LocalSoftwareKeyboardController.current
    val isKeyboardVisible = WindowInsets.isImeVisible

    var showRequested by remember { mutableStateOf(false) }

    LaunchedEffect(isVisible) {
        showRequested = isVisible
    }

    LaunchedEffect(showRequested, isKeyboardVisible) {
        if (showRequested && isKeyboardVisible) {
            keyboardController?.hide()
        }
    }

    val effectivelyVisible = showRequested && !isKeyboardVisible

    content(effectivelyVisible)
}
