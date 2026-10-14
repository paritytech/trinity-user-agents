import Foundation
import UIKit
import UIKitExt

@MainActor
protocol ChatExtensionWireframeProtocol: AnyObject {
    var view: ControllerBackedProtocol? { get set }

    @discardableResult
    func present(error: Error) -> Bool
}

extension ChatExtensionWireframeProtocol where Self: AlertPresentable & ErrorPresentable {
    @discardableResult
    func present(error: Error) -> Bool {
        present(error: error, from: view)
    }
}

@MainActor
protocol ChatExtensionNavigating: AnyObject {
    var botSettings: ChatExtensionBotSettings { get }
    var application: UIApplication { get }
    var registryDelegate: ChatExtensionDelegate? { get set }

    func openChatWithExtension(_ extensionId: ChatExtension.Id)
    func openLink(_ link: URL)
}

extension ChatExtensionNavigating {
    func openChatWithExtension(_ extensionId: ChatExtension.Id) {
        defer {
            let chatId = Chat.Id.chatExtension(extensionId)
            application.open(AppConfig.DeepLink.chat(chatId, force: true))
        }
        guard !botSettings.isEnabled(extId: extensionId) else {
            return
        }
        botSettings.set(enabled: true, for: extensionId)
        registryDelegate?.didEnableExtensions([extensionId])
    }

    func openLink(_ link: URL) {
        application.open(link)
    }
}
