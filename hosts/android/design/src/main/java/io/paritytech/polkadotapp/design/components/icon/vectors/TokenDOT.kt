package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

val NovaIcons.TokenDOT: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "DOT",
        defaultWidth = 48.dp,
        defaultHeight = 48.dp,
        viewportWidth = 48f,
        viewportHeight = 48f
    ).apply {
        path(fill = SolidColor(Color.White)) {
            moveTo(24f, 0f)
            lineTo(24f, 0f)
            arcTo(24f, 24f, 0f, isMoreThanHalf = false, isPositiveArc = true, 48f, 24f)
            lineTo(48f, 24f)
            arcTo(24f, 24f, 0f, isMoreThanHalf = false, isPositiveArc = true, 24f, 48f)
            lineTo(24f, 48f)
            arcTo(24f, 24f, 0f, isMoreThanHalf = false, isPositiveArc = true, 0f, 24f)
            lineTo(0f, 24f)
            arcTo(24f, 24f, 0f, isMoreThanHalf = false, isPositiveArc = true, 24f, 0f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFF2670))) {
            moveTo(24.045f, 13.056f)
            curveTo(27.841f, 13.056f, 30.918f, 11.278f, 30.918f, 9.085f)
            curveTo(30.918f, 6.892f, 27.841f, 5.114f, 24.045f, 5.114f)
            curveTo(20.25f, 5.114f, 17.172f, 6.892f, 17.172f, 9.085f)
            curveTo(17.172f, 11.278f, 20.25f, 13.056f, 24.045f, 13.056f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFF2670))) {
            moveTo(24.045f, 42.96f)
            curveTo(27.841f, 42.96f, 30.918f, 41.182f, 30.918f, 38.989f)
            curveTo(30.918f, 36.796f, 27.841f, 35.019f, 24.045f, 35.019f)
            curveTo(20.25f, 35.019f, 17.172f, 36.796f, 17.172f, 38.989f)
            curveTo(17.172f, 41.182f, 20.25f, 42.96f, 24.045f, 42.96f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFF2670))) {
            moveTo(14.466f, 18.546f)
            curveTo(16.364f, 15.282f, 16.352f, 11.748f, 14.44f, 10.651f)
            curveTo(12.527f, 9.555f, 9.438f, 11.312f, 7.54f, 14.576f)
            curveTo(5.642f, 17.84f, 5.654f, 21.374f, 7.567f, 22.47f)
            curveTo(9.479f, 23.566f, 12.568f, 21.809f, 14.466f, 18.546f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFF2670))) {
            moveTo(40.553f, 33.498f)
            curveTo(42.451f, 30.235f, 42.438f, 26.7f, 40.526f, 25.604f)
            curveTo(38.613f, 24.507f, 35.523f, 26.264f, 33.625f, 29.528f)
            curveTo(31.728f, 32.791f, 31.74f, 36.326f, 33.653f, 37.423f)
            curveTo(35.566f, 38.519f, 38.655f, 36.762f, 40.553f, 33.498f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFF2670))) {
            moveTo(14.44f, 37.423f)
            curveTo(16.353f, 36.327f, 16.365f, 32.792f, 14.467f, 29.529f)
            curveTo(12.569f, 26.265f, 9.48f, 24.508f, 7.567f, 25.605f)
            curveTo(5.654f, 26.701f, 5.642f, 30.236f, 7.54f, 33.499f)
            curveTo(9.437f, 36.763f, 12.527f, 38.52f, 14.44f, 37.423f)
            close()
        }
        path(fill = SolidColor(Color(0xFFFF2670))) {
            moveTo(40.525f, 22.47f)
            curveTo(42.438f, 21.374f, 42.45f, 17.839f, 40.552f, 14.575f)
            curveTo(38.654f, 11.312f, 35.565f, 9.555f, 33.652f, 10.651f)
            curveTo(31.739f, 11.748f, 31.727f, 15.283f, 33.625f, 18.546f)
            curveTo(35.523f, 21.81f, 38.612f, 23.567f, 40.525f, 22.47f)
            close()
        }
    }.build()
}
