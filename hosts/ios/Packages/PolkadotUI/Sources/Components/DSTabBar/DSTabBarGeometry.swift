import UIKit

enum DSTabBarMetrics {
    static let capsuleHeight: CGFloat = 62
    static let itemHeight: CGFloat = 54
    static let innerInset: CGFloat = 4
    static let rowPadding: CGFloat = 2
    static let itemOverlap: CGFloat = 0
    static let horizontalMargin: CGFloat = 21
    static let bottomGap: CGFloat = 21
    static let maxWidth: CGFloat = 500
    static let iconSize: CGFloat = 28
    static let itemTopPadding: CGFloat = 6
    static let itemBottomPadding: CGFloat = 7
    static let pillHorizontalInset: CGFloat = 0
    static let badgeDiameter: CGFloat = 8
    static let liftedScaleX: CGFloat = 1.08
    static let liftedScaleY: CGFloat = 1.3
    static let foldedVisibleWidth: CGFloat = 19
    static let foldFlickVelocity: CGFloat = 800
    static let foldFlickMinTravel: CGFloat = 20
    static let foldGrabZoneWidth: CGFloat = 44
    static let foldSettleFraction: CGFloat = 0.3
    static let foldTapSlop: CGFloat = 10
    static let foldFlickMaxSampleAge: TimeInterval = 0.05
    static let selectionCancelVerticalSlop: CGFloat = 44
    static let selectionSpringDuration: TimeInterval = 0.4
    static let selectionSpringDamping: CGFloat = 0.8

    static var capsuleCornerRadius: CGFloat { capsuleHeight / 2 }
    static var pillCornerRadius: CGFloat { itemHeight / 2 }
}

enum DSTabBarGeometry {
    static func capsuleWidth(availableWidth: CGFloat) -> CGFloat {
        let inset = availableWidth - DSTabBarMetrics.horizontalMargin * 2
        return max(0, min(inset, DSTabBarMetrics.maxWidth))
    }

    static func capsuleFrame(availableWidth: CGFloat) -> CGRect {
        let width = capsuleWidth(availableWidth: availableWidth)
        return CGRect(
            x: ((availableWidth - width) / 2).rounded(),
            y: 0,
            width: width,
            height: DSTabBarMetrics.capsuleHeight
        )
    }

    static func foldedTranslationX(availableWidth: CGFloat) -> CGFloat {
        DSTabBarMetrics.foldedVisibleWidth - capsuleFrame(availableWidth: availableWidth).maxX
    }

    static func hiddenTranslationX(availableWidth: CGFloat) -> CGFloat {
        -capsuleFrame(availableWidth: availableWidth).maxX
    }

    static func rowWidth(capsuleWidth: CGFloat) -> CGFloat {
        max(0, capsuleWidth - DSTabBarMetrics.innerInset * 2)
    }

    static func clampedPillOriginX(_ xPosition: CGFloat, pillWidth: CGFloat, rowWidth: CGFloat) -> CGFloat {
        max(0, min(xPosition, rowWidth - pillWidth))
    }
}

struct DSTabBarRow {
    let width: CGFloat
    let itemCount: Int
}

extension DSTabBarRow {
    func itemFrame(at index: Int) -> CGRect {
        CGRect(
            x: DSTabBarMetrics.rowPadding + CGFloat(index) * unitStride,
            y: 0,
            width: unitWidth,
            height: DSTabBarMetrics.itemHeight
        )
    }

    func pillFrame(at index: Int) -> CGRect {
        itemFrame(at: index).insetBy(dx: DSTabBarMetrics.pillHorizontalInset, dy: 0)
    }

    /// Snaps to the nearest centre among `candidates` only, so a drag sweeping over an action item
    /// passes it by. Returns `nil` when there is nothing to snap to.
    func nearestItemIndex(toX xPosition: CGFloat, restrictedTo candidates: [Int]) -> Int? {
        candidates.min {
            abs(itemFrame(at: $0).midX - xPosition) < abs(itemFrame(at: $1).midX - xPosition)
        }
    }
}

private extension DSTabBarRow {
    var unitWidth: CGFloat {
        guard itemCount > 0 else {
            return 0
        }
        let usable = width - DSTabBarMetrics.rowPadding * 2
        let overlapTotal = CGFloat(itemCount - 1) * DSTabBarMetrics.itemOverlap
        return max(0, (usable + overlapTotal) / CGFloat(itemCount))
    }

    var unitStride: CGFloat {
        unitWidth - DSTabBarMetrics.itemOverlap
    }
}
