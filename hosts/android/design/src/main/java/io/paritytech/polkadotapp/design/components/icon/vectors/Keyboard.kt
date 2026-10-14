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

val NovaIcons.Keyboard: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Keyboard",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(19.875f, 4.563f)
            horizontalLineTo(4.125f)
            curveTo(3.162f, 4.563f, 2.375f, 5.35f, 2.375f, 6.313f)
            verticalLineTo(17.688f)
            curveTo(2.375f, 18.65f, 3.162f, 19.438f, 4.125f, 19.438f)
            horizontalLineTo(19.875f)
            curveTo(20.837f, 19.438f, 21.625f, 18.65f, 21.625f, 17.688f)
            verticalLineTo(6.313f)
            curveTo(21.625f, 5.35f, 20.837f, 4.563f, 19.875f, 4.563f)
            close()
            moveTo(19.875f, 17.688f)
            horizontalLineTo(4.125f)
            verticalLineTo(6.313f)
            horizontalLineTo(19.875f)
            verticalLineTo(17.688f)
            close()
            moveTo(9.375f, 8.063f)
            horizontalLineTo(11.125f)
            verticalLineTo(9.813f)
            horizontalLineTo(9.375f)
            verticalLineTo(8.063f)
            close()
            moveTo(5.875f, 8.063f)
            horizontalLineTo(7.625f)
            verticalLineTo(9.813f)
            horizontalLineTo(5.875f)
            verticalLineTo(8.063f)
            close()
            moveTo(8.5f, 15.063f)
            horizontalLineTo(15.5f)
            verticalLineTo(15.938f)
            horizontalLineTo(8.5f)
            verticalLineTo(15.063f)
            close()
            moveTo(12.875f, 8.063f)
            horizontalLineTo(14.625f)
            verticalLineTo(9.813f)
            horizontalLineTo(12.875f)
            verticalLineTo(8.063f)
            close()
            moveTo(9.375f, 11.563f)
            horizontalLineTo(11.125f)
            verticalLineTo(13.313f)
            horizontalLineTo(9.375f)
            verticalLineTo(11.563f)
            close()
            moveTo(5.875f, 11.563f)
            horizontalLineTo(7.625f)
            verticalLineTo(13.313f)
            horizontalLineTo(5.875f)
            verticalLineTo(11.563f)
            close()
            moveTo(12.875f, 11.563f)
            horizontalLineTo(14.625f)
            verticalLineTo(13.313f)
            horizontalLineTo(12.875f)
            verticalLineTo(11.563f)
            close()
            moveTo(16.375f, 8.063f)
            horizontalLineTo(18.125f)
            verticalLineTo(9.813f)
            horizontalLineTo(16.375f)
            verticalLineTo(8.063f)
            close()
            moveTo(16.375f, 11.563f)
            horizontalLineTo(18.125f)
            verticalLineTo(13.313f)
            horizontalLineTo(16.375f)
            verticalLineTo(11.563f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun KeyboardPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.Keyboard, contentDescription = null)
    }
}
