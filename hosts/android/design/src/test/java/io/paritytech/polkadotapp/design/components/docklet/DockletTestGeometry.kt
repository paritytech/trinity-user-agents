package io.paritytech.polkadotapp.design.components.docklet

import org.junit.Assert.assertEquals

internal const val GeometryEps = 1e-3f

internal fun assertPoint(x: Float, y: Float, actual: DockletPoint) {
    assertEquals(x, actual.x, GeometryEps)
    assertEquals(y, actual.y, GeometryEps)
}

internal fun assertCurves(expected: List<List<Pair<Float, Float>>>, actual: List<CubicSegment>) {
    assertEquals(expected.size, actual.size)
    expected.zip(actual).forEach { (points, segment) ->
        listOf(segment.control1, segment.control2, segment.end).zip(points).forEach { (point, expectedPoint) ->
            assertPoint(expectedPoint.first, expectedPoint.second, point)
        }
    }
}

internal fun cubicPoint(start: DockletPoint, segment: CubicSegment, t: Float): DockletPoint {
    val u = 1f - t
    fun axis(p0: Float, p1: Float, p2: Float, p3: Float) =
        u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3
    return DockletPoint(
        x = axis(start.x, segment.control1.x, segment.control2.x, segment.end.x),
        y = axis(start.y, segment.control1.y, segment.control2.y, segment.end.y),
    )
}
