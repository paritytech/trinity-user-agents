import DesignSystem
import PolkadotUI
import UIKit

/// The height the scan panel declares for its content at a given panel width, including the grabber's
/// clearance at the top, which the scan layouts provide. Declared rather than measured:
/// measuring a live scanner would start a second capture session. The camera is square, as
/// `EmbeddedQRScannerViewLayout` pins it, and the rows around it are fixed in both scan layouts.
enum ScanPanelContentHeight {
    /// Inset the camera keeps on each side in both scan layouts.
    private static let cameraSideInset = DSSpacings.mediumIncreased
    /// Height of the search row in the embedded layout. Its 48pt capsule sits 8pt inside each edge of the row.
    static let embeddedSearchRowHeight: CGFloat = 64
    /// Height of the search button in the plain layout.
    static let plainSearchRowHeight: CGFloat = 48

    static func height(forPanelWidth width: CGFloat, searchRowHeight: CGFloat) -> CGFloat {
        let topInset = DSTabBarStyle.centerGrabberHeight + DSSpacings.mediumIncreased
        let gapBelowContent = DSSpacings.small
        let bottomInset = DSSpacings.small
        let cameraSide = width - 2 * cameraSideInset
        return topInset + cameraSide + gapBelowContent + searchRowHeight + bottomInset
    }
}
