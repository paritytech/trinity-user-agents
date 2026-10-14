import UIKit
import Foundation_iOS
public import UIKit_iOS
import ExternalAccessibility
import FoundationExt

public final class TitleDetailsSheetViewController: UIViewController, ViewHolder {
    public typealias RootViewType = TitleDetailsSheetViewLayout

    let presenter: MessageSheetPresenterProtocol
    let viewModel: TitleDetailsSheetViewModel
    let styler: MessageSheetStyling

    public var allowsSwipeDown: Bool = true
    public var closeOnSwipeDownClosure: (() -> Void)?

    init(
        presenter: MessageSheetPresenterProtocol,
        viewModel: TitleDetailsSheetViewModel,
        styler: MessageSheetStyling,
        localizationManager: LocalizationManagerProtocol? = nil
    ) {
        self.presenter = presenter
        self.styler = styler
        self.viewModel = viewModel

        super.init(nibName: nil, bundle: nil)

        self.localizationManager = localizationManager
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func loadView() {
        view = TitleDetailsSheetViewLayout(controlFactory: styler.controlFactory)
    }

    override public func viewDidLoad() {
        super.viewDidLoad()

        setupHandlers()
        applyStyle()
        setupLocalization()
    }

    private func setupLocalization() {
        if let graphics = viewModel.graphics {
            rootView.setupGraphicsView()

            rootView.graphicsView?.image = graphics
        }

        rootView.titleLabel.text = viewModel.title.value(for: selectedLocale)
        let textMode = viewModel.message.value(for: selectedLocale)

        switch textMode {
        case let .normal(string):
            rootView.detailsLabel.text = string
        case let .attributed(attributedString):
            rootView.detailsLabel.attributedText = attributedString
        }

        if let action = viewModel.mainAction {
            rootView.mainActionButton?.setTitle(action.title.value(for: selectedLocale))
            rootView.mainActionButton?.accessibilityId(rawValue: action.accessibilityId)
        }

        if let action = viewModel.secondaryAction {
            rootView.secondaryActionButton?.setTitle(action.title.value(for: selectedLocale))
            rootView.secondaryActionButton?.accessibilityId(rawValue: action.accessibilityId)
        }

        if let action = viewModel.tertiaryAction {
            rootView.tertiaryActionButton?.setTitle(action.title.value(for: selectedLocale))
            rootView.tertiaryActionButton?.accessibilityId(rawValue: action.accessibilityId)
        }
    }

    private func setupHandlers() {
        if viewModel.mainAction != nil {
            rootView.setupMainActionButton()
            rootView.mainActionButton?.addTarget(self, action: #selector(actionMain), for: .touchUpInside)
        }

        if viewModel.secondaryAction != nil {
            rootView.setupSecondaryActionButton()
            rootView.secondaryActionButton?.addTarget(self, action: #selector(actionSecondary), for: .touchUpInside)
        }

        if viewModel.tertiaryAction != nil {
            rootView.setupTertiaryActionButton()
            rootView.tertiaryActionButton?.addTarget(self, action: #selector(actionTertiary), for: .touchUpInside)
        }
    }

    private func applyStyle() {
        styler.applyStyle(to: rootView)
    }

    @objc private func actionMain() {
        presenter.goBack(with: viewModel.mainAction)
    }

    @objc private func actionSecondary() {
        presenter.goBack(with: viewModel.secondaryAction)
    }

    @objc private func actionTertiary() {
        presenter.goBack(with: viewModel.tertiaryAction)
    }
}

extension TitleDetailsSheetViewController: MessageSheetViewProtocol {}

extension TitleDetailsSheetViewController: ModalPresenterDelegate {
    public func presenterShouldHide(_: ModalPresenterProtocol) -> Bool {
        allowsSwipeDown
    }

    public func presenterDidHide(_: ModalPresenterProtocol) {
        closeOnSwipeDownClosure?()
    }
}

extension TitleDetailsSheetViewController: Localizable {
    public func applyLocalization() {
        if isViewLoaded {
            setupLocalization()
        }
    }
}
