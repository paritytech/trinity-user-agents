import Foundation
import UIKitExt

public protocol MessageSheetViewProtocol: ControllerBackedProtocol {}

public protocol MessageSheetGraphicsProtocol {
    associatedtype GraphicsViewModel

    func bind(messageSheetGraphics: GraphicsViewModel?, locale: Locale)
}

public protocol MessageSheetContentProtocol {
    associatedtype ContentViewModel

    func bind(messageSheetContent: ContentViewModel?, locale: Locale)
}

@MainActor
protocol MessageSheetPresenterProtocol: AnyObject {
    func goBack(with action: MessageSheetAction?)
}

protocol MessageSheetInteractorInputProtocol: AnyObject {}

protocol MessageSheetInteractorOutputProtocol: AnyObject {}

@MainActor
protocol MessageSheetWireframeProtocol: AnyObject {
    func complete(on view: MessageSheetViewProtocol?, with action: MessageSheetAction?)
}

public typealias MessageSheetCallback = () -> Void
