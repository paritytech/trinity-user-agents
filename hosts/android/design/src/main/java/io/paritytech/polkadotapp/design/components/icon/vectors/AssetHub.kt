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

val NovaIcons.AssetHub: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "AssetHub",
        defaultWidth = 10.dp,
        defaultHeight = 10.dp,
        viewportWidth = 10f,
        viewportHeight = 10f
    ).apply {
        path(fill = SolidColor(Color(0xFF000000))) {
            moveTo(0.156f, 2.773f)
            curveTo(0.776f, 2.57f, 1.461f, 2.305f, 2.078f, 2.083f)
            curveTo(2.158f, 1.898f, 2.252f, 1.609f, 2.322f, 1.414f)
            lineTo(2.777f, 0.155f)
            lineTo(4.297f, 0.447f)
            curveTo(4.447f, 0.476f, 4.857f, 0.562f, 4.993f, 0.571f)
            curveTo(5.085f, 0.577f, 5.543f, 0.477f, 5.671f, 0.452f)
            curveTo(6.189f, 0.351f, 6.708f, 0.252f, 7.227f, 0.156f)
            lineTo(7.921f, 2.083f)
            lineTo(9.847f, 2.775f)
            curveTo(9.766f, 3.276f, 9.641f, 3.8f, 9.553f, 4.301f)
            curveTo(9.529f, 4.44f, 9.434f, 4.856f, 9.433f, 4.977f)
            curveTo(9.432f, 5.137f, 9.531f, 5.585f, 9.565f, 5.759f)
            lineTo(9.847f, 7.218f)
            curveTo(9.829f, 7.236f, 8.101f, 7.855f, 7.924f, 7.919f)
            curveTo(7.687f, 8.513f, 7.444f, 9.234f, 7.228f, 9.844f)
            curveTo(6.722f, 9.767f, 6.178f, 9.633f, 5.669f, 9.546f)
            curveTo(5.522f, 9.521f, 5.15f, 9.434f, 5.017f, 9.431f)
            curveTo(4.879f, 9.429f, 4.475f, 9.52f, 4.323f, 9.548f)
            curveTo(3.808f, 9.643f, 3.291f, 9.756f, 2.776f, 9.843f)
            lineTo(2.36f, 8.688f)
            curveTo(2.273f, 8.448f, 2.174f, 8.15f, 2.076f, 7.918f)
            curveTo(1.453f, 7.681f, 0.785f, 7.451f, 0.155f, 7.225f)
            curveTo(0.283f, 6.512f, 0.464f, 5.717f, 0.569f, 4.996f)
            curveTo(0.58f, 4.919f, 0.47f, 4.41f, 0.449f, 4.304f)
            curveTo(0.349f, 3.794f, 0.251f, 3.283f, 0.156f, 2.773f)
            close()
            moveTo(5.001f, 0.588f)
            curveTo(4.664f, 1.707f, 4.296f, 2.854f, 3.974f, 3.976f)
            curveTo(3.177f, 4.197f, 2.367f, 4.472f, 1.57f, 4.704f)
            curveTo(1.452f, 4.738f, 0.667f, 4.968f, 0.604f, 5.007f)
            curveTo(0.99f, 5.114f, 1.392f, 5.242f, 1.776f, 5.359f)
            lineTo(3.977f, 6.027f)
            curveTo(4.301f, 7.15f, 4.662f, 8.291f, 5.002f, 9.411f)
            lineTo(6.027f, 6.028f)
            lineTo(8.319f, 5.332f)
            lineTo(9.006f, 5.124f)
            curveTo(9.113f, 5.091f, 9.297f, 5.04f, 9.392f, 4.998f)
            curveTo(9.065f, 4.88f, 8.65f, 4.771f, 8.311f, 4.667f)
            lineTo(6.029f, 3.974f)
            lineTo(5.341f, 1.71f)
            curveTo(5.231f, 1.348f, 5.101f, 0.95f, 5.001f, 0.588f)
            close()
        }
    }.build()
}

@Preview(showBackground = true)
@Composable
private fun AssetHubPreview() {
    Box(modifier = Modifier.padding(12.dp)) {
        Image(imageVector = NovaIcons.AssetHub, contentDescription = null)
    }
}
