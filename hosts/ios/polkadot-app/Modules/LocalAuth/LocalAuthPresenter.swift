import Foundation

@MainActor
final class LocalAuthPresenter {
    weak var view: LocalAuthViewProtocol?
    let wireframe: LocalAuthWireframeProtocol
    let interactor: LocalAuthInteractorInputProtocol

    let logger: LoggerProtocol
    let retriable: Bool

    init(
        interactor: LocalAuthInteractorInputProtocol,
        wireframe: LocalAuthWireframeProtocol,
        retriable: Bool,
        logger: LoggerProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
        self.retriable = retriable
        self.logger = logger
    }

    private func startAuth() {
        view?.didStartAuth()
        interactor.startAuth(with: String(localized: .askAuthReason))
    }
}

extension LocalAuthPresenter: LocalAuthPresenterProtocol {
    func setup() {
        startAuth()
    }

    func retryAuth() {
        startAuth()
    }
}

extension LocalAuthPresenter: LocalAuthInteractorOutputProtocol {
    func didCompleteAuth() {
        logger.debug("Auth completed")

        wireframe.complete(with: true)
    }

    func didFailedAuth(with error: DeviceAuthError) {
        logger.debug("Auth failed: \(error)")

        switch error {
        case .authFailed,
             .notAvailable,
             .unknown:
            wireframe.showAuthFailed(from: view) { [weak self] in
                guard let self else {
                    return
                }

                if retriable {
                    view?.didStopAuth()
                } else {
                    wireframe.complete(with: false)
                }
            }
        case .cancelled:
            if retriable {
                view?.didStopAuth()
            } else {
                wireframe.complete(with: false)
            }
        }
    }

    func didInterruptAuth() {
        logger.debug("Auth cancelled")

        if retriable {
            view?.didStopAuth()
        } else {
            wireframe.complete(with: false)
        }
    }
}
