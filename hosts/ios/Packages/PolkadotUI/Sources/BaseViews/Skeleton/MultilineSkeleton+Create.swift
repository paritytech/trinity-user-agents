import Foundation
public import UIKit_iOS
import UIKit

public extension MultilineSkeleton {
    static func createRows(
        inPlaceOf targetView: UIView,
        containerView: UIView,
        spaceSize: CGSize,
        lineSize: CGSize,
        spacing: CGFloat,
        lastLineFraction: CGFloat = 0.5
    ) -> MultilineSkeleton {
        let targetFrame = targetView.convert(targetView.bounds, to: containerView)

        let position = CGPoint(
            x: targetFrame.minX + lineSize.width / 2.0,
            y: targetFrame.minY + lineSize.height / 2.0
        )

        let mappedLineSize = CGSize(
            width: spaceSize.skrullMapX(lineSize.width),
            height: spaceSize.skrullMapY(lineSize.height)
        )

        let lineHeightWithSpacing = lineSize.height + spacing

        let count: UInt =
            if lineHeightWithSpacing > 0 {
                UInt((targetFrame.height + spacing) / lineHeightWithSpacing)
            } else {
                0
            }

        return MultilineSkeleton(
            startLinePosition: spaceSize.skrullMap(point: position),
            lineSize: mappedLineSize,
            count: UInt8(min(count, UInt(UInt8.max))),
            spacing: spaceSize.skrullMapY(spacing)
        ).round().lastLine(fraction: lastLineFraction)
    }
}
