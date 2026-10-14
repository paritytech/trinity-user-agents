import UIKitExt

protocol TransactionFailureViewProtocol: ControllerBackedProtocol, AlertPresentable {}

@MainActor
protocol TransactionFailurePresenterProtocol: AnyObject {
    func setup()
    func onAppear()
    func onAction()
}

@MainActor
protocol TransactionFailureWireframeProtocol: AnyObject {
    func hide(view: TransactionFailureViewProtocol?)
}

typealias TransactionFailureCompletion = () -> Void
