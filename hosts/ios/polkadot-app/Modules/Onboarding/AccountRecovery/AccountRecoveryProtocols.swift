import Foundation
import Foundation_iOS
import UIKitExt

protocol AccountRecoveryViewProtocol: ControllerBackedProtocol {
    func didReceive(inputViewModel: InputViewModelProtocol)
}

@MainActor
protocol AccountRecoveryPresenterProtocol: AnyObject {
    func setup()
    func proceed()
}

protocol AccountRecoveryInteractorInputProtocol: AnyObject {
    func proceed(withWords words: String)
}

@MainActor
protocol AccountRecoveryInteractorOutputProtocol: AnyObject {
    func didRestoreWallets()
    func didReceiveInvalidMnemonicFormat()
    func didDecideBroken()
    func authorizeUser(completion: @escaping AuthorizationCompletionBlock)
}

@MainActor
protocol AccountRecoveryWireframeProtocol: BottomSheetErrorPresentable, AuthorizationPresentable {
    func didDecideBroken()
    func didRestoreWallets()
}
