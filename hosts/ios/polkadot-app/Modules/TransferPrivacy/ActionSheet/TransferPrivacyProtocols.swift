import UIKitExt

protocol TransferPrivacyViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: TransferPrivacyViewModel)
}

@MainActor
protocol TransferPrivacyPresenterProtocol: AnyObject {
    func setup()
    func sendAnyway()
    func cancel()
}

@MainActor
protocol TransferPrivacyWireframeProtocol: AnyObject {
    func complete(from view: TransferPrivacyViewProtocol?, _ completion: (() -> Void)?)
}
