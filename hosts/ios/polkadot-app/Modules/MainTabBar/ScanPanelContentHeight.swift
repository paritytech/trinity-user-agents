import DesignSystem
import UIKit

/// The height the scan panel declares for its content at a given panel width. Declared rather than measured:
/// measuring a live scanner would start a second capture session. The camera is square, as
/// `EmbeddedQRScannerViewLayout` pins it, and the rows around it are fixed in both scan layouts. While the
/// search is focused the camera collapses and the results take its place, so the height follows the results.
enum ScanPanelContentHeight {
    /// Inset the camera keeps on each side in both scan layouts.
    private static let cameraSideInset = DSSpacings.mediumIncreased
    /// Height of the search row in the embedded layout. Its 48pt capsule sits 8pt inside each edge of the row.
    static let embeddedSearchRowHeight: CGFloat = 64
    /// Height of the search button in the plain layout.
    static let plainSearchRowHeight: CGFloat = 48

    /// `resultsHeight` only counts while the search is focused, when the results sit where the camera does.
    static func height(
        forPanelWidth width: CGFloat,
        searchRowHeight: CGFloat,
        isSearchFocused: Bool = false,
        resultsHeight: CGFloat = 0
    ) -> CGFloat {
        let gapBelowContent = DSSpacings.small
        let bottomInset = DSSpacings.small

        guard isSearchFocused else {
            let topInset = DSSpacings.mediumIncreased
            let cameraSide = width - 2 * cameraSideInset
            return topInset + cameraSide + gapBelowContent + searchRowHeight + bottomInset
        }

        let topInset = DSSpacings.tiny
        return topInset + resultsHeight + gapBelowContent + searchRowHeight + bottomInset
    }
}
