import UIKitExt

protocol RestoreFromCloudViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: RestoreFromCloudViewLayout.ViewModel)
}

@MainActor
protocol RestoreFromCloudPresenterProtocol: AnyObject {
    func setup()
    func viewDidAppear()
}

protocol RestoreFromCloudInteractorInputProtocol: AnyObject {
    func restoreWallets()
}

@MainActor
protocol RestoreFromCloudInteractorOutputProtocol: AnyObject {
    func didReceiveInProgress(_ value: Bool)
    func didFailAuthorization()
    func didRestoreWallets()
    func didDecideBroken()
    func authorizeUser(completion: @escaping AuthorizationCompletionBlock)
}

@MainActor
protocol RestoreFromCloudWireframeProtocol: AuthorizationPresentable {
    var observer: RootStateObserving { get }
}
