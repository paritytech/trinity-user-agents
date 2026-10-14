package io.paritytech.polkadotapp.design.components.icon.vectors

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.path
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcons

private const val CENTRE = 14f
private const val OUTER_RADIUS = 10.4077f
private const val MIDDLE_RADIUS = 7.15039f
private const val CORE_RADIUS = 3.5459f
private const val HALO_ALPHA = 0.4f

// The two halos are drawn middle-then-outer, so the middle band is covered twice and lands at 0.64
// while the outer stays at 0.4 — the three-step ramp the design specifies. Reordering these flattens it.
val NovaIcons.NetworkStatus: ImageVector by lazy(LazyThreadSafetyMode.NONE) {
    ImageVector.Builder(
        name = "NetworkStatus",
        defaultWidth = 28.dp,
        defaultHeight = 28.dp,
        viewportWidth = 28f,
        viewportHeight = 28f
    ).apply {
        circle(radius = MIDDLE_RADIUS, alpha = HALO_ALPHA)
        circle(radius = OUTER_RADIUS, alpha = HALO_ALPHA)
        circle(radius = CORE_RADIUS, alpha = 1f)
    }.build()
}

private fun ImageVector.Builder.circle(radius: Float, alpha: Float) {
    path(fill = SolidColor(Color.Black), fillAlpha = alpha) {
        moveTo(CENTRE, CENTRE)
        moveToRelative(-radius, 0f)
        arcToRelative(radius, radius, 0f, isMoreThanHalf = true, isPositiveArc = true, 2 * radius, 0f)
        arcToRelative(radius, radius, 0f, isMoreThanHalf = true, isPositiveArc = true, -2 * radius, 0f)
    }
}
