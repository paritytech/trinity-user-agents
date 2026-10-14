import UIKit
import SnapKit
import PolkadotUI

struct AppWidgetID: Hashable {
    let rawValue: String

    init(_ rawValue: String) {
        self.rawValue = rawValue
    }
}

protocol AppWidgetManaging: AnyObject {
    func attachWidget(_ configuration: any HashableContentConfiguration, for id: AppWidgetID)
    func detachWidget(for id: AppWidgetID)
}

final class AppWidgetContentViewController: UIViewController {
    private var widgetConfiguration: any HashableContentConfiguration
    private var widgetContentView: (UIView & UIContentView)?
    private var widgetContentReuseIdentifier: String?

    init(configuration: any HashableContentConfiguration) {
        widgetConfiguration = configuration
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = AppWidgetPassthroughView()
    }

    override func viewDidLoad() {
        super.viewDidLoad()

        view.backgroundColor = .clear
        apply(configuration: widgetConfiguration)
    }

    func update(configuration: any HashableContentConfiguration) {
        widgetConfiguration = configuration

        guard isViewLoaded else {
            return
        }

        apply(configuration: configuration)
    }

    private func apply(configuration: any HashableContentConfiguration) {
        if let widgetContentView,
           widgetContentReuseIdentifier == configuration.defaultReuseIdentifier {
            widgetContentView.configuration = configuration
            widgetContentView.invalidateIntrinsicContentSize()
            view.invalidateIntrinsicContentSize()
            return
        }

        widgetContentView?.removeFromSuperview()

        let contentView = configuration.makeContentView()
        view.addSubview(contentView)

        contentView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }

        widgetContentView = contentView
        widgetContentReuseIdentifier = configuration.defaultReuseIdentifier
        view.invalidateIntrinsicContentSize()
    }
}

private final class AppWidgetPassthroughView: UIView {
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        let hitView = super.hitTest(point, with: event)
        return hitView === self ? nil : hitView
    }
}
