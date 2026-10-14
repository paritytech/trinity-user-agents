import Foundation
import UIKit_iOS
import PolkadotUI

extension LoadableActionView {
    func applyMainStyle() {
        actionLoadingView.backgroundView.applyBackgroundStyle(
            .bgActionPrimary,
            cornerRadius: 12
        )
        actionLoadingView.activityIndicator.color = .fgPrimaryInverted
        actionButton.applyMainStyle()
        tintColor = .fgPrimaryInverted
    }
}
