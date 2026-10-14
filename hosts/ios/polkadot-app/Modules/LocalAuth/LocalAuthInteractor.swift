import UIKit

final class LocalAuthInteractor {
    weak var presenter: LocalAuthInteractorOutputProtocol?

    let deviceAuth: DeviceAuthProtocol

    init(deviceAuth: DeviceAuthProtocol) {
        self.deviceAuth = deviceAuth
    }
}

extension LocalAuthInteractor: LocalAuthInteractorInputProtocol {
    func startAuth(with reason: String) {
        guard deviceAuth.isAvailable else {
            MainActor.assumeIsolated {
                presenter?.didFailedAuth(with: .notAvailable)
            }
            return
        }

        deviceAuth.authenticate(
            localizedReason: reason,
            completionQueue: .main
        ) { [weak self] result in
            MainActor.assumeIsolated {
                switch result {
                case let .success(authorized):
                    if authorized {
                        self?.presenter?.didCompleteAuth()
                    } else {
                        self?.presenter?.didInterruptAuth()
                    }
                case let .failure(error):
                    self?.presenter?.didFailedAuth(with: error)
                }
            }
        }
    }
}
