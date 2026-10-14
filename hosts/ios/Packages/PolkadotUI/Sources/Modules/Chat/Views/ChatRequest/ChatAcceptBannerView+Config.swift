import Foundation
import UIKit
import SwiftUI
import DesignSystem

extension ChatAcceptBannerView.ViewModel: @preconcurrency ChatInputViewConfigurationProtocol {
    public var activateOnAppear: Bool {
        false
    }

    public var safeAreaFillColor: UIColor? {
        UIColor.bgSurfaceContainer
    }

    @MainActor
    public func makeContentView(for _: ChatInputHandling?) -> UIView {
        UIHostingConfiguration {
            ChatAcceptBannerView(viewModel: self)
        }
        .margins(.all, 0)
        .makeContentView()
    }

    public func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
        guard let otherViewModel = configuration as? ChatAcceptBannerView.ViewModel else {
            return false
        }

        return username == otherViewModel.username
    }
}
