package io.paritytech.polkadotapp.feature_chats_api.presentation.common

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp

@Composable
fun getMaxMessageWidth(): Dp {
    return LocalWindowInfo.current.containerDpSize.width * 0.8f
}

@Composable
fun getMaxMessageHeight(): Dp {
    return LocalWindowInfo.current.containerDpSize.height / 3f
}
