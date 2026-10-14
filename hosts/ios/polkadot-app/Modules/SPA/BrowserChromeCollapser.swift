import CoreGraphics

struct BrowserChromeScrollSample: Equatable {
    let offsetToTopEdge: CGFloat
    let isInteracting: Bool
}

struct BrowserChromeCollapseUpdate: Equatable {
    let fraction: CGFloat
    let animated: Bool
}

final class BrowserChromeCollapser {
    private let topPanelHeight: CGFloat

    private(set) var fraction: CGFloat = 0
    private var previousOffsetToTop: CGFloat?

    init(topPanelHeight: CGFloat) {
        self.topPanelHeight = topPanelHeight
    }

    func update(with sample: BrowserChromeScrollSample) -> BrowserChromeCollapseUpdate? {
        guard let previous = previousOffsetToTop else {
            previousOffsetToTop = sample.offsetToTopEdge
            return nil
        }
        let offsetDelta = sample.offsetToTopEdge - previous
        previousOffsetToTop = sample.offsetToTopEdge

        var newFraction = fraction
        if topPanelHeight > 0 {
            newFraction = min(1, max(0, newFraction + offsetDelta / topPanelHeight))
        }

        if topPanelHeight > 0, sample.offsetToTopEdge < topPanelHeight {
            newFraction = min(newFraction, sample.offsetToTopEdge / topPanelHeight)
        }

        var animated = false
        if !sample.isInteracting {
            newFraction = newFraction < 0.5 ? 0 : 1
            animated = true
        }

        guard newFraction != fraction else { return nil }
        fraction = newFraction
        return BrowserChromeCollapseUpdate(fraction: newFraction, animated: animated)
    }

    func reset() {
        fraction = 0
        previousOffsetToTop = nil
    }
}
