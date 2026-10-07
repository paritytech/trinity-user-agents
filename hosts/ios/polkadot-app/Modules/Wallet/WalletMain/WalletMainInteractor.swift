import Foundation

final class WalletMainInteractor {
    weak var presenter: WalletMainInteractorOutputProtocol?

    private let networkStatusObserver: NetworkStatusObserving
    private var resolutionTask: Task<Void, Never>?

    init(
        networkStatusObserver: NetworkStatusObserving
    ) {
        self.networkStatusObserver = networkStatusObserver
    }

    deinit {
        resolutionTask?.cancel()
    }
}

extension WalletMainInteractor: WalletMainInteractorInputProtocol {
    func setup() {
        guard resolutionTask == nil else { return }

        networkStatusObserver.start { [weak self] status in
            self?.presenter?.didReceive(networkStatus: status)
        }
    }
}
