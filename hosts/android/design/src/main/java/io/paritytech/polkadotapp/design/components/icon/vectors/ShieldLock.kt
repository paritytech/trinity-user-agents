package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathFillType
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.ShieldLock: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ShieldLock",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // Shackle and body are separate subpaths that meet exactly at y = 11.2 — they must not overlap, or
        // EvenOdd would fill the intersection back in instead of leaving a hole.
        path(fill = SolidColor(Color(0xFF000000)), pathFillType = PathFillType.EvenOdd) {
            moveTo(12f, 2f)
            lineTo(20f, 5f)
            verticalLineTo(11.5f)
            curveTo(20f, 16.5f, 16.6f, 20.9f, 12f, 22f)
            curveTo(7.4f, 20.9f, 4f, 16.5f, 4f, 11.5f)
            verticalLineTo(5f)
            close()

            moveTo(9.4f, 11.2f)
            verticalLineTo(10f)
            curveTo(9.4f, 8.56f, 10.56f, 7.4f, 12f, 7.4f)
            curveTo(13.44f, 7.4f, 14.6f, 8.56f, 14.6f, 10f)
            verticalLineTo(11.2f)
            horizontalLineTo(13.2f)
            verticalLineTo(10f)
            curveTo(13.2f, 9.34f, 12.66f, 8.8f, 12f, 8.8f)
            curveTo(11.34f, 8.8f, 10.8f, 9.34f, 10.8f, 10f)
            verticalLineTo(11.2f)
            close()

            moveTo(8.6f, 11.2f)
            horizontalLineTo(15.4f)
            verticalLineTo(16.2f)
            horizontalLineTo(8.6f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun ShieldLockPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.ShieldLock, contentDescription = null)
    }
}
