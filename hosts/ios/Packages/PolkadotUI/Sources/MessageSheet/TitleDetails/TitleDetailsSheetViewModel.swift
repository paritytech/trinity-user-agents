import Foundation
import Foundation_iOS
import UIKit

public struct TitleDetailsSheetViewModel {
    public enum Text {
        case normal(String)
        case attributed(NSAttributedString)
    }

    let graphics: UIImage?
    let title: LocalizableResource<String>
    let message: LocalizableResource<Text>
    let mainAction: MessageSheetAction?
    let secondaryAction: MessageSheetAction?
    let tertiaryAction: MessageSheetAction?

    public init(
        graphics: UIImage?,
        title: LocalizableResource<String>,
        message: LocalizableResource<Text>,
        mainAction: MessageSheetAction?,
        secondaryAction: MessageSheetAction?,
        tertiaryAction: MessageSheetAction? = nil
    ) {
        self.graphics = graphics
        self.title = title
        self.message = message
        self.mainAction = mainAction
        self.secondaryAction = secondaryAction
        self.tertiaryAction = tertiaryAction
    }
}
