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

val NovaIcons.People: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "People",
        defaultWidth = 10.dp,
        defaultHeight = 10.dp,
        viewportWidth = 10f,
        viewportHeight = 10f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(9.085f, 9.625f)
            horizontalLineTo(0.915f)
            verticalLineTo(5.569f)
            horizontalLineTo(9.085f)
            verticalLineTo(9.625f)
            close()
            moveTo(5f, 0.375f)
            curveTo(6.268f, 0.375f, 7.296f, 1.403f, 7.296f, 2.672f)
            curveTo(7.296f, 3.94f, 6.268f, 4.969f, 5f, 4.969f)
            curveTo(3.731f, 4.969f, 2.703f, 3.94f, 2.703f, 2.672f)
            curveTo(2.703f, 1.403f, 3.731f, 0.375f, 5f, 0.375f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun PeoplePreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.People, contentDescription = null)
    }
}
