package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

data class BrowserChromeScrollSample(
    val offsetToTopEdge: Float,
    val isInteracting: Boolean,
)

data class BrowserChromeCollapseUpdate(
    val fraction: Float,
    val animated: Boolean,
)

// Mirrors the iOS BrowserChromeCollapser, so the chrome above a product hides and returns the same way on both.
class BrowserChromeCollapser(private val topPanelHeight: Float) {
    var fraction: Float = 0f
        private set

    private var previousOffsetToTop: Float? = null

    fun update(sample: BrowserChromeScrollSample): BrowserChromeCollapseUpdate? {
        val previous = previousOffsetToTop
        previousOffsetToTop = sample.offsetToTopEdge
        if (previous == null) return null

        var newFraction = fraction
        if (topPanelHeight > 0) {
            newFraction = (newFraction + (sample.offsetToTopEdge - previous) / topPanelHeight).coerceIn(0f, 1f)

            if (sample.offsetToTopEdge < topPanelHeight) {
                newFraction = minOf(newFraction, sample.offsetToTopEdge / topPanelHeight)
            }
        }

        var animated = false
        if (!sample.isInteracting) {
            newFraction = if (newFraction < 0.5f) 0f else 1f
            animated = true
        }

        if (newFraction == fraction) return null
        fraction = newFraction
        return BrowserChromeCollapseUpdate(newFraction, animated)
    }
}
