import UIKitExt

protocol TransactionSuccessViewProtocol: ControllerBackedProtocol {}

@MainActor
protocol TransactionSuccessPresenterProtocol: AnyObject {
    func setup()
    func onAppear()
    func activateDone()
}

@MainActor
protocol TransactionSuccessWireframeProtocol: AnyObject {
    func hide(view: TransactionSuccessViewProtocol?)
}

typealias TransactionSuccessCompletion = () -> Void
