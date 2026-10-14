import PolkadotUI
import UIKitExt

protocol PolkadotSignInViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: PolkadotSignInViewLayout.ViewModel)
}

@MainActor
protocol PolkadotSignInPresenterProtocol: AnyObject {
    func setup()
    func approve()
    func cancel()
}

protocol PolkadotSignInInteractorInputProtocol: AnyObject {
    func setup()
    func approve(with input: HandshakeInput)
}

@MainActor
protocol PolkadotSignInInteractorOutputProtocol: AnyObject {
    func didStartFetchingInput()
    func didFinishFetchingInput(_ input: HandshakeInput)
    func didFailToFetchInput(with error: Error)

    func didStartSendingHandshake()
    func didFinishSendingHandshake(with device: Chat.LocalDevice)
    func didFailToSendHandshake(with error: Error)
}

enum PolkadotSignInResult {
    case success(Chat.LocalDevice)
    case noFreeSlots(message: String)
    case failed
}

@MainActor
protocol PolkadotSignInWireframeProtocol: AnyObject {
    func hide(view: PolkadotSignInViewProtocol?, with result: PolkadotSignInResult?)
}
