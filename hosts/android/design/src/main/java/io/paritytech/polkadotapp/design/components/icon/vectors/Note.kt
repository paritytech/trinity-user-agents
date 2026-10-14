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

val NovaIcons.Note: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Note",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(15.5f, 5f)
            horizontalLineTo(5f)
            curveTo(4.037f, 5f, 3.25f, 5.787f, 3.25f, 6.75f)
            verticalLineTo(17.259f)
            curveTo(3.25f, 18.221f, 4.037f, 19f, 5f, 19f)
            horizontalLineTo(19f)
            curveTo(19.962f, 19f, 20.75f, 18.212f, 20.75f, 17.25f)
            verticalLineTo(10.25f)
            lineTo(15.5f, 5f)
            close()
            moveTo(5f, 17.259f)
            verticalLineTo(6.75f)
            horizontalLineTo(14.625f)
            verticalLineTo(11.125f)
            horizontalLineTo(19f)
            verticalLineTo(17.259f)
            horizontalLineTo(5f)
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
