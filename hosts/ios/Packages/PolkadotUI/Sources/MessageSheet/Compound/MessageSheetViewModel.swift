import Foundation
import Foundation_iOS
import ExternalAccessibility
import UIKit

public struct MessageSheetAction {
    public enum ActionType {
        case normal
        case destructive
    }

    let title: LocalizableResource<String>
    let handler: MessageSheetCallback
    let actionType: ActionType
    let accessibilityId: String?

    public init(
        title: LocalizableResource<String>,
        handler: @escaping MessageSheetCallback,
        actionType: ActionType = .normal,
        accessibilityId: (any AccessibilityIdentifying)? = nil
    ) {
        self.title = title
        self.handler = handler
        self.actionType = actionType
        self.accessibilityId = accessibilityId?.rawValue
    }
}

public enum MessageSheetText {
    case raw(String)
    case attributed(NSAttributedString)
}

public struct MessageSheetViewModel<IType, CType> {
    let title: LocalizableResource<String>
    let message: LocalizableResource<MessageSheetText>
    let graphics: IType?
    let content: CType?
    let mainAction: MessageSheetAction?
    let secondaryAction: MessageSheetAction?

    public init(
        title: LocalizableResource<String>,
        message: LocalizableResource<MessageSheetText>,
        graphics: IType?,
        content: CType?,
        mainAction: MessageSheetAction?,
        secondaryAction: MessageSheetAction?
    ) {
        self.title = title
        self.message = message
        self.graphics = graphics
        self.content = content
        self.mainAction = mainAction
        self.secondaryAction = secondaryAction
    }

    public init(
        title: LocalizableResource<String>,
        message: LocalizableResource<String>,
        graphics: IType?,
        content: CType?,
        mainAction: MessageSheetAction?,
        secondaryAction: MessageSheetAction?
    ) {
        self.title = title
        self.message = LocalizableResource { locale in
            let string = message.value(for: locale)
            return .raw(string)
        }
        self.graphics = graphics
        self.content = content
        self.mainAction = mainAction
        self.secondaryAction = secondaryAction
    }

    public init(
        title: LocalizableResource<String>,
        message: LocalizableResource<NSAttributedString>,
        graphics: IType?,
        content: CType?,
        mainAction: MessageSheetAction?,
        secondaryAction: MessageSheetAction?
    ) {
        self.title = title
        self.message = LocalizableResource { locale in
            let attributedText = message.value(for: locale)
            return .attributed(attributedText)
        }
        self.graphics = graphics
        self.content = content
        self.mainAction = mainAction
        self.secondaryAction = secondaryAction
    }
}
