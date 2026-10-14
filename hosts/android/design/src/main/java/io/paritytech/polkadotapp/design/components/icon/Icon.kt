package io.paritytech.polkadotapp.design.components.icon

import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.vector.ImageVector

@Composable
fun NovaIcon(
    modifier: Modifier = Modifier,
    painter: Painter,
    contentDescription: String? = null,
    tint: Color = LocalContentColor.current
) {
    Icon(
        modifier = modifier,
        painter = painter,
        contentDescription = contentDescription,
        tint = tint
    )
}

@Composable
fun NovaIcon(
    modifier: Modifier = Modifier,
    bitmap: ImageBitmap,
    contentDescription: String? = null,
    tint: Color = LocalContentColor.current
) {
    Icon(
        modifier = modifier,
        bitmap = bitmap,
        contentDescription = contentDescription,
        tint = tint
    )
}

@Composable
fun NovaIcon(
    modifier: Modifier = Modifier,
    imageVector: ImageVector,
    contentDescription: String? = null,
    tint: Color = LocalContentColor.current
) {
    Icon(
        modifier = modifier,
        imageVector = imageVector,
        contentDescription = contentDescription,
        tint = tint
    )
}
