package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.MarkChatUnread: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "MarkChatUnread",
        defaultWidth = 12.dp,
        defaultHeight = 12.dp,
        viewportWidth = 12f,
        viewportHeight = 12f
    ).apply {
        path(fill = SolidColor(Color.Black)) {
            moveTo(11f, 3.99f)
            verticalLineTo(8.5f)
            curveTo(11f, 9.05f, 10.55f, 9.5f, 10f, 9.5f)
            horizontalLineTo(3f)
            lineTo(1f, 11.5f)
            verticalLineTo(2.5f)
            curveTo(1f, 1.95f, 1.45f, 1.5f, 2f, 1.5f)
            horizontalLineTo(7.05f)
            curveTo(7.02f, 1.66f, 7f, 1.83f, 7f, 2f)
            curveTo(7f, 3.38f, 8.12f, 4.5f, 9.5f, 4.5f)
            curveTo(10.065f, 4.5f, 10.58f, 4.305f, 11f, 3.99f)
            close()
            moveTo(8f, 2f)
            curveTo(8f, 2.83f, 8.67f, 3.5f, 9.5f, 3.5f)
            curveTo(10.33f, 3.5f, 11f, 2.83f, 11f, 2f)
            curveTo(11f, 1.17f, 10.33f, 0.5f, 9.5f, 0.5f)
            curveTo(8.67f, 0.5f, 8f, 1.17f, 8f, 2f)
            close()
        }
    }.build()
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun MarkChatUnreadPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.MarkChatUnread, contentDescription = null)
    }
}
