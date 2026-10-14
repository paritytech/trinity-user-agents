import Foundation
import ChainRegistry
import PolkadotUI

protocol NetworkStatusTitleViewModelMaking {
    func createTitleViewModel(for networkStatus: NetworkStatus) -> NetworkStatusTitleView.ViewModel
}

final class NetworkStatusTitleViewModelFactory {
    private let screenTitle: String

    init(screenTitle: String) {
        self.screenTitle = screenTitle
    }
}

extension NetworkStatusTitleViewModelFactory: NetworkStatusTitleViewModelMaking {
    func createTitleViewModel(for networkStatus: NetworkStatus) -> NetworkStatusTitleView.ViewModel {
        switch networkStatus {
        case .connected:
            .init(text: screenTitle, isLoading: false)
        case .connecting:
            .init(text: String(localized: .networkStatusConnecting), isLoading: true)
        case .waitingForNetwork:
            .init(text: String(localized: .networkStatusWaiting), isLoading: true)
        }
    }
}
