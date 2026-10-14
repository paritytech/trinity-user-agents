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

val NovaIcons.ShieldOutlined: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ShieldOutlined",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // The rim is the filled silhouette with a scaled copy of itself punched out, so the outline shares
        // the exact shape of `ShieldCheck` and `ShieldLock` instead of being a second, subtly different
        // shield.
        path(fill = SolidColor(Color(0xFF000000)), pathFillType = PathFillType.EvenOdd) {
            moveTo(12f, 2f)
            lineTo(20f, 5f)
            verticalLineTo(11.5f)
            curveTo(20f, 16.5f, 16.6f, 20.9f, 12f, 22f)
            curveTo(7.4f, 20.9f, 4f, 16.5f, 4f, 11.5f)
            verticalLineTo(5f)
            close()

            moveTo(12f, 3.8f)
            lineTo(18.6f, 6.3f)
            verticalLineTo(11.6f)
            curveTo(18.6f, 15.7f, 15.8f, 19.3f, 12f, 20.2f)
            curveTo(8.2f, 19.3f, 5.4f, 15.7f, 5.4f, 11.6f)
            verticalLineTo(6.3f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun ShieldOutlinedPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.ShieldOutlined, contentDescription = null)
    }
}
