import UIKitExt

protocol LocalAuthViewProtocol: ControllerBackedProtocol {
    func didStartAuth()
    func didStopAuth()
}

@MainActor
protocol LocalAuthPresenterProtocol: AnyObject {
    func setup()
    func retryAuth()
}

protocol LocalAuthInteractorInputProtocol: AnyObject {
    func startAuth(with reason: String)
}

@MainActor
protocol LocalAuthInteractorOutputProtocol: AnyObject {
    func didCompleteAuth()
    func didInterruptAuth()
    func didFailedAuth(with error: DeviceAuthError)
}

@MainActor
protocol LocalAuthWireframeProtocol: AnyObject {
    func complete(with isSuccess: Bool)
    func showAuthFailed(from view: LocalAuthViewProtocol?, completion: @escaping () -> Void)
}
