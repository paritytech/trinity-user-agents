import DesignSystem
import UIKit

/// The height the scan panel declares for its content at a given panel width. Declared rather than measured:
/// measuring a live scanner would start a second capture session. The camera is square, as
/// `EmbeddedQRScannerViewLayout` pins it, and the rows around it are fixed in both scan layouts.
enum ScanPanelContentHeight {
    /// Inset the camera keeps on each side in both scan layouts.
    private static let cameraSideInset = DSSpacings.mediumIncreased
    /// Height of the search row that both layouts place under the camera.
    private static let searchRowHeight: CGFloat = 48

    static func height(forPanelWidth width: CGFloat) -> CGFloat {
        let topInset = DSSpacings.mediumIncreased
        let cameraSide = width - 2 * cameraSideInset
        let gapBelowCamera = DSSpacings.small
        let bottomInset = DSSpacings.small

        return topInset + cameraSide + gapBelowCamera + searchRowHeight + bottomInset
    }
}
