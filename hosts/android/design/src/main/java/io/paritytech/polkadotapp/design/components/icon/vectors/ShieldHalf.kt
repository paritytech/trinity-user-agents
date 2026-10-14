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

val NovaIcons.ShieldHalf: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "ShieldHalf",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // Half a shield is the whole silhouette with its right half hollowed out: the second subpath is the
        // same outline scaled towards the centre and cut at the axis, so EvenOdd leaves a rim on the right
        // and solid fill on the left. Sharing the outline with `Shield` keeps the two the same shape.
        path(fill = SolidColor(Color(0xFF000000)), pathFillType = PathFillType.EvenOdd) {
            moveTo(12f, 2f)
            lineTo(20f, 5f)
            verticalLineTo(11.5f)
            curveTo(20f, 16.5f, 16.6f, 20.9f, 12f, 22f)
            curveTo(7.4f, 20.9f, 4f, 16.5f, 4f, 11.5f)
            verticalLineTo(5f)
            close()

            moveTo(12f, 4.8f)
            lineTo(17.8f, 7f)
            verticalLineTo(11.6f)
            curveTo(17.8f, 15.2f, 15.3f, 18.4f, 12f, 19.2f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun ShieldHalfPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.ShieldHalf, contentDescription = null)
    }
}
