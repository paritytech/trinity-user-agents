package io.paritytech.polkadotapp.design.components.navigationbar

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

private val NotificationDotSize = 8.dp

@Composable
internal fun NotificationDot(
    modifier: Modifier,
    visible: Boolean,
) {
    AnimatedVisibility(
        modifier = modifier,
        visible = visible,
        enter = scaleIn() + fadeIn(),
        exit = scaleOut() + fadeOut()
    ) {
        Box(
            modifier = Modifier
                .size(NotificationDotSize)
                .background(
                    color = PolkadotTheme.colors.bg.status.warning,
                    shape = CircleShape
                )
        )
    }
}
