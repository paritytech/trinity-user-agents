import Foundation
import UIKit
import SwiftUI
import DesignSystem

extension ChatRequestedBannerView.ViewModel: @preconcurrency ChatInputViewConfigurationProtocol {
    public var activateOnAppear: Bool {
        false
    }

    public var safeAreaFillColor: UIColor? {
        UIColor.bgSurfaceContainer
    }

    @MainActor
    public func makeContentView(for _: ChatInputHandling?) -> UIView {
        UIHostingConfiguration {
            ChatRequestedBannerView(viewModel: self)
        }
        .margins(.all, 0)
        .makeContentView()
    }

    public func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
        guard let otherViewModel = configuration as? ChatRequestedBannerView.ViewModel else {
            return false
        }

        return username == otherViewModel.username
    }
}
