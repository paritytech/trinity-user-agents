import DesignSystem
import PolkadotUI
import UIKit

/// The height the scan panel declares for its content at a given panel width, including the grabber's
/// clearance at the top, which the scan layout provides. Declared rather than measured:
/// measuring a live scanner would start a second capture session. The camera is square, as
/// `EmbeddedQRScannerViewLayout` pins it, and the rows around it are fixed in the scan layout.
enum ScanPanelContentHeight {
    /// Inset the camera keeps on each side in the scan layout.
    private static let cameraSideInset = DSSpacings.mediumIncreased
    /// Height of the search row. Its 48pt capsule sits 8pt inside each edge of the row.
    private static let searchRowHeight: CGFloat = 64

    static func height(forPanelWidth width: CGFloat) -> CGFloat {
        let topInset = DSTabBarStyle.centerGrabberHeight + DSSpacings.mediumIncreased
        let gapBelowContent = DSSpacings.small
        let bottomInset = DSSpacings.small
        let cameraSide = width - 2 * cameraSideInset
        return topInset + cameraSide + gapBelowContent + searchRowHeight + bottomInset
    }
}
