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

val NovaIcons.ShieldCheck: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ShieldCheck",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // The check is a second subpath rather than a stroke, so EvenOdd punches it out of the shield and the
        // icon stays a single tintable shape.
        path(fill = SolidColor(Color(0xFF000000)), pathFillType = PathFillType.EvenOdd) {
            moveTo(12f, 2f)
            lineTo(20f, 5f)
            verticalLineTo(11.5f)
            curveTo(20f, 16.5f, 16.6f, 20.9f, 12f, 22f)
            curveTo(7.4f, 20.9f, 4f, 16.5f, 4f, 11.5f)
            verticalLineTo(5f)
            close()

            moveTo(10.9f, 15.7f)
            lineTo(16.4f, 10.2f)
            lineTo(15f, 8.8f)
            lineTo(10.9f, 12.9f)
            lineTo(9f, 11f)
            lineTo(7.6f, 12.4f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun ShieldCheckPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.ShieldCheck, contentDescription = null)
    }
}
