import UIKit

enum TabBarScrollToTopLocator {
    static func scrollView(in root: UIView) -> UIScrollView? {
        var found: [ScrollToTopCandidate] = []
        collect(in: root, root: root, depth: 0, into: &found)

        guard let widest = found.map(\.visibleArea).max() else {
            return nil
        }

        return found
            .filter { $0.visibleArea >= widest * dominanceRatio }
            .min(by: ScrollToTopCandidate.isOrderedBefore)?
            .scrollView
    }

    static func scrollToTop(_ scrollView: UIScrollView) {
        let top = -scrollView.adjustedContentInset.top

        guard scrollView.contentOffset.y > top + tolerance else {
            return
        }

        scrollView.setContentOffset(CGPoint(x: scrollView.contentOffset.x, y: top), animated: true)
    }
}

private extension TabBarScrollToTopLocator {
    static var tolerance: CGFloat { 0.5 }
    static var dominanceRatio: CGFloat { 0.5 }
    static var minimumAlpha: CGFloat { 0.01 }

    static func collect(
        in view: UIView,
        root: UIView,
        depth: Int,
        into found: inout [ScrollToTopCandidate]
    ) {
        guard !view.isHidden, view.alpha > minimumAlpha else {
            return
        }

        if let scrollView = view as? UIScrollView,
           let candidate = makeCandidate(for: scrollView, root: root, depth: depth) {
            found.append(candidate)
        }

        for subview in view.subviews {
            collect(in: subview, root: root, depth: depth + 1, into: &found)
        }
    }

    static func makeCandidate(for scrollView: UIScrollView, root: UIView, depth: Int) -> ScrollToTopCandidate? {
        guard scrollView.scrollsToTop, let area = visibleArea(of: scrollView, in: root) else {
            return nil
        }

        let inset = scrollView.adjustedContentInset
        let content = scrollView.contentSize
        let bounds = scrollView.bounds
        let scrollsVertically = content.height > bounds.height - inset.top - inset.bottom + tolerance
        let scrollsHorizontally = content.width > bounds.width - inset.left - inset.right + tolerance

        guard scrollsVertically || !scrollsHorizontally else {
            return nil
        }

        return ScrollToTopCandidate(
            scrollView: scrollView,
            visibleArea: area,
            scrollsVertically: scrollsVertically,
            depth: depth
        )
    }

    static func visibleArea(of view: UIView, in root: UIView) -> CGFloat? {
        let visible = view.convert(view.bounds, to: root).intersection(root.bounds)

        guard !visible.isNull, visible.width > tolerance, visible.height > tolerance else {
            return nil
        }

        return visible.width * visible.height
    }
}

private struct ScrollToTopCandidate {
    let scrollView: UIScrollView
    let visibleArea: CGFloat
    let scrollsVertically: Bool
    let depth: Int

    static func isOrderedBefore(_ lhs: Self, _ rhs: Self) -> Bool {
        if lhs.scrollsVertically != rhs.scrollsVertically {
            return lhs.scrollsVertically
        }

        if lhs.visibleArea != rhs.visibleArea {
            return lhs.visibleArea > rhs.visibleArea
        }

        return lhs.depth < rhs.depth
    }
}
