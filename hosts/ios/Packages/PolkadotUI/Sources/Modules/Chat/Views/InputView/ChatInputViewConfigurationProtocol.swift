import UIKit

public protocol ChatInputViewConfigurationProtocol {
    var activateOnAppear: Bool { get }
    var safeAreaFillColor: UIColor? { get }
    /// When an input that also animates replacement gives way to this one, the two crossfade
    /// and the chat's bottom inset follows the height change in the same animation.
    var animatesReplacement: Bool { get }

    func makeContentView(for handler: ChatInputHandling?) -> UIView
    func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool
    /// Calls `ready` once `view`, made by `makeContentView`, shows its content. An animated
    /// replacement keeps the input it replaces on screen until then.
    func whenContentReady(_ view: UIView, _ ready: @escaping () -> Void)
}

public extension ChatInputViewConfigurationProtocol {
    var safeAreaFillColor: UIColor? { nil }

    var animatesReplacement: Bool { false }

    func whenContentReady(_: UIView, _ ready: @escaping () -> Void) {
        ready()
    }
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
