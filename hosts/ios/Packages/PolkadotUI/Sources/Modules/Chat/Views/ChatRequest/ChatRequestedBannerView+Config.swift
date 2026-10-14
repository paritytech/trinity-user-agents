import Foundation
import UIKit
import SwiftUI

extension ChatRequestedBannerView.ViewModel: @preconcurrency ChatInputViewConfigurationProtocol {
    public var activateOnAppear: Bool {
        false
    }

    @MainActor
    public func makeContentView(for _: ChatInputHandling?) -> UIView {
        UIHostingConfiguration {
            ChatRequestedBannerView(viewModel: self)
        }
        .margins(.vertical, 0)
        .makeContentView()
    }

    public func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
        guard let otherViewModel = configuration as? ChatRequestedBannerView.ViewModel else {
            return false
        }

        return username == otherViewModel.username
    }
}
