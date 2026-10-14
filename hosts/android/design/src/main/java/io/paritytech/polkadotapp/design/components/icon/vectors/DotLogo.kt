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

val NovaIcons.DotLogo: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "Logo",
        defaultWidth = 24.dp,
        defaultHeight = 24.dp,
        viewportWidth = 24f,
        viewportHeight = 24f
    ).apply {
        path(fill = SolidColor(Color(0xFFE6007A))) {
            moveTo(0f, 12f)
            curveTo(0f, 18.627f, 5.373f, 24f, 12f, 24f)
            curveTo(18.627f, 24f, 24f, 18.627f, 24f, 12f)
            curveTo(24f, 5.373f, 18.627f, 0f, 12f, 0f)
            curveTo(5.373f, 0f, 0f, 5.373f, 0f, 12f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(12f, 7.091f)
            curveTo(13.686f, 7.091f, 15.052f, 6.296f, 15.052f, 5.315f)
            curveTo(15.052f, 4.335f, 13.686f, 3.54f, 12f, 3.54f)
            curveTo(10.315f, 3.54f, 8.949f, 4.335f, 8.949f, 5.315f)
            curveTo(8.949f, 6.296f, 10.315f, 7.091f, 12f, 7.091f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(12f, 20.463f)
            curveTo(13.686f, 20.463f, 15.052f, 19.668f, 15.052f, 18.688f)
            curveTo(15.052f, 17.707f, 13.686f, 16.912f, 12f, 16.912f)
            curveTo(10.315f, 16.912f, 8.949f, 17.707f, 8.949f, 18.688f)
            curveTo(8.949f, 19.668f, 10.315f, 20.463f, 12f, 20.463f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(7.747f, 9.546f)
            curveTo(8.59f, 8.087f, 8.584f, 6.506f, 7.735f, 6.016f)
            curveTo(6.886f, 5.526f, 5.515f, 6.311f, 4.672f, 7.771f)
            curveTo(3.83f, 9.23f, 3.835f, 10.811f, 4.684f, 11.301f)
            curveTo(5.533f, 11.791f, 6.905f, 11.006f, 7.747f, 9.546f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(19.328f, 16.233f)
            curveTo(20.171f, 14.773f, 20.166f, 13.193f, 19.316f, 12.702f)
            curveTo(18.467f, 12.212f, 17.096f, 12.998f, 16.253f, 14.457f)
            curveTo(15.411f, 15.917f, 15.416f, 17.497f, 16.265f, 17.987f)
            curveTo(17.114f, 18.478f, 18.486f, 17.692f, 19.328f, 16.233f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(7.735f, 17.987f)
            curveTo(8.584f, 17.497f, 8.59f, 15.916f, 7.747f, 14.457f)
            curveTo(6.905f, 12.998f, 5.533f, 12.212f, 4.684f, 12.702f)
            curveTo(3.835f, 13.192f, 3.83f, 14.773f, 4.672f, 16.232f)
            curveTo(5.515f, 17.692f, 6.886f, 18.477f, 7.735f, 17.987f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFFFFFF))) {
            moveTo(19.316f, 11.301f)
            curveTo(20.166f, 10.811f, 20.171f, 9.23f, 19.328f, 7.771f)
            curveTo(18.486f, 6.311f, 17.114f, 5.526f, 16.265f, 6.016f)
            curveTo(15.416f, 6.506f, 15.411f, 8.087f, 16.253f, 9.546f)
            curveTo(17.096f, 11.006f, 18.467f, 11.791f, 19.316f, 11.301f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun LogoPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.DotLogo, contentDescription = null)
    }
}
