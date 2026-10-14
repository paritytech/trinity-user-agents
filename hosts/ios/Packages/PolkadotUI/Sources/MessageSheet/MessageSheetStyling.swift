import UIKit
import Foundation
public import UIKit_iOS

public enum MessageSheetAxis {
    case horizontal
    case vertical
}

public enum MessageSheetButtonsOrder {
    case mainSecondary
    case secondaryMain
}

public protocol MessageSheetStyleAcceptable: AnyObject {
    var backgroundView: RoundedView { get }
    var titleLabel: Label { get }
    var detailsLabel: Label { get }
    var backgroundInsets: UIEdgeInsets { get set }
    var contentInsets: UIEdgeInsets { get set }
    var afterGraphicsSpacing: CGFloat { get set }
    var afterTitleSpacing: CGFloat { get set }
    var afterDetailsSpacing: CGFloat { get set }
    var buttonsSpacing: CGFloat { get set }
    var buttonsAxis: MessageSheetAxis { get set }
    var buttonsOrder: MessageSheetButtonsOrder { get set }
    var actionHeight: CGFloat { get set }
}

public protocol MessageSheetControlProtocol {
    func setTitle(_ title: String)
}

public typealias MessageSheetControl = MessageSheetControlProtocol & UIControl

public protocol MessageSheetControlFactoryProtocol {
    func createMain() -> MessageSheetControl
    func createSecondary() -> MessageSheetControl
    func createTertiary() -> MessageSheetControl
}

public extension MessageSheetControlFactoryProtocol {
    func createTertiary() -> MessageSheetControl {
        createSecondary()
    }
}

public protocol MessageSheetStyling {
    var controlFactory: MessageSheetControlFactoryProtocol { get }

    func applyStyle(to view: MessageSheetStyleAcceptable)
}

extension RoundedButton: MessageSheetControlProtocol {}

extension DSButtonView: MessageSheetControlProtocol {}

extension GradientButton: MessageSheetControlProtocol {
    public func setTitle(_ title: String) {
        imageWithTitleView?.title = title
        invalidateLayout()
    }
}
