import Foundation
import PolkadotUI
import UIKitExt

protocol ShareViewProtocol: ControllerBackedProtocol {
    func didReceive(viewModel: ShareViewLayout.ViewModel)
}

@MainActor
protocol SharePresenterProtocol: AnyObject {
    func setup()
    func didTapShare()
    func didTapCancel()
    func didTapSystemShare()
    func didToggleSelection(chatId: Chat.Id, isSelected: Bool)
}

protocol ShareInteractorInputProtocol: AnyObject {
    func setup()
    func send(items: [ShareItem], userMessage: String?, to chatIds: [Chat.Id])
}

@MainActor
protocol ShareInteractorOutputProtocol: AnyObject {
    func didReceive(chats: [ChatWithPeerMetadata])
    func didReceive(error: Error)
    func didReceive(isLoading: Bool)
    func didCompleteSend()
}

@MainActor
protocol ShareWireframeProtocol: AlertPresentable, ErrorPresentable {
    func close(from view: ControllerBackedProtocol?)
    func presentSystemShare(items: [ShareItem], from view: ControllerBackedProtocol?)
}
