import FoundationExt
import UIKit
import UIKitExt

final class ExpandedPocketCardViewController: UIViewController, ViewHolder {
    typealias RootViewType = ExpandedPocketCardViewLayout

    let presenter: ExpandedPocketCardPresenterProtocol

    private let cardInset: CGFloat
    private let schemeHandlerProxy: SchemeHandlerProxy

    init(
        presenter: ExpandedPocketCardPresenterProtocol,
        cardInset: CGFloat,
        schemeHandlerProxy: SchemeHandlerProxy
    ) {
        self.presenter = presenter
        self.cardInset = cardInset
        self.schemeHandlerProxy = schemeHandlerProxy
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = ExpandedPocketCardViewLayout(
            widgetView: WidgetView(cardInset: cardInset, schemeHandler: schemeHandlerProxy)
        )
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        presenter.setup()
    }
}

extension ExpandedPocketCardViewController: ExpandedPocketCardViewProtocol {}
