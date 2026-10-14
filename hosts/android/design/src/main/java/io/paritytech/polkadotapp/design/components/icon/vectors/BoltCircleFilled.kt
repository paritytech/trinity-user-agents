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

val NovaIcons.BoltCircleFilled: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "BoltCircleFilled",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        // The bolt is punched out rather than drawn, so whatever the disc sits on shows through it — on the
        // privacy-mode circle that is the mode's own accent gradient, which is what the design asks for.
        // The disc is full-bleed: inset the way the rest of the set is, it would read a size smaller than
        // the mockup inside the circle. The glyph is `Bolt` scaled to 0.6 about the centre, so both icons
        // stay the same bolt.
        path(fill = SolidColor(Color(0xFF000000)), pathFillType = PathFillType.EvenOdd) {
            moveTo(12f, 0f)
            curveTo(5.373f, 0f, 0f, 5.373f, 0f, 12f)
            curveTo(0f, 18.627f, 5.373f, 24f, 12f, 24f)
            curveTo(18.627f, 24f, 24f, 18.627f, 24f, 12f)
            curveTo(24f, 5.373f, 18.627f, 0f, 12f, 0f)
            close()

            moveTo(12.9f, 6f)
            lineTo(8.1f, 12.9f)
            horizontalLineTo(11.7f)
            lineTo(11.1f, 18f)
            lineTo(15.9f, 11.1f)
            horizontalLineTo(12.3f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun BoltCircleFilledPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.BoltCircleFilled, contentDescription = null)
    }
}
