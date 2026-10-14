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

val NovaIcons.Bulletin: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Bulletin",
        defaultWidth = 10.dp,
        defaultHeight = 10.dp,
        viewportWidth = 10f,
        viewportHeight = 10f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(0.931f, 1.476f)
            horizontalLineTo(3.52f)
            lineTo(4.572f, 2.76f)
            horizontalLineTo(9.069f)
            verticalLineTo(8.524f)
            horizontalLineTo(0.931f)
            verticalLineTo(1.476f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun BulletinPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.Bulletin, contentDescription = null)
    }
}
