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

val NovaIcons.Block: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Block",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFFFF3123))) {
            moveTo(12f, 2f)
            curveTo(6.48f, 2f, 2f, 6.48f, 2f, 12f)
            curveTo(2f, 17.52f, 6.48f, 22f, 12f, 22f)
            curveTo(17.52f, 22f, 22f, 17.52f, 22f, 12f)
            curveTo(22f, 6.48f, 17.52f, 2f, 12f, 2f)
            close()
            moveTo(4f, 12f)
            curveTo(4f, 7.58f, 7.58f, 4f, 12f, 4f)
            curveTo(13.85f, 4f, 15.55f, 4.63f, 16.9f, 5.69f)
            lineTo(5.69f, 16.9f)
            curveTo(4.63f, 15.55f, 4f, 13.85f, 4f, 12f)
            close()
            moveTo(12f, 20f)
            curveTo(10.15f, 20f, 8.45f, 19.37f, 7.1f, 18.31f)
            lineTo(18.31f, 7.1f)
            curveTo(19.37f, 8.45f, 20f, 10.15f, 20f, 12f)
            curveTo(20f, 16.42f, 16.42f, 20f, 12f, 20f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun SheetPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.Note, contentDescription = null)
    }
}
