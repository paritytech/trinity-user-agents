import UIKit

public protocol ChatInputViewConfigurationProtocol {
    var activateOnAppear: Bool { get }
    var safeAreaFillColor: UIColor? { get }

    func makeContentView(for handler: ChatInputHandling?) -> UIView
    func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool
}

public extension ChatInputViewConfigurationProtocol {
    var safeAreaFillColor: UIColor? { nil }
}

public protocol ChatInputReplyPresenting: AnyObject {
    func showReplyBanner(title: String, messageText: String)
    func hideReplyBanner()
}

public protocol ChatInputEditPresenting: AnyObject {
    var isEditing: Bool { get }

    func showEditBanner(title: String, currentText: String)
    func hideEditBanner()
}

public protocol ChatTextInputPresenting: AnyObject {
    func activateTextField()
}
