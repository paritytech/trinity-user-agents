import UIKit
public import UIKit_iOS

public final class TitleDetailsSheetViewLayout: UIView {
    public let titleLabel: Label = .create {
        $0.numberOfLines = 0
    }

    public let detailsLabel: Label = .create {
        $0.numberOfLines = 0
    }

    private(set) var graphicsView: UIImageView?
    private(set) var mainActionButton: MessageSheetControl?
    private(set) var secondaryActionButton: MessageSheetControl?
    private(set) var tertiaryActionButton: MessageSheetControl?
    private(set) var buttonsStackView: UIStackView?

    private var titleDetailsStackView: UIStackView?

    public private(set) var backgroundView = RoundedView()

    private let layoutView: UIView = .create { view in
        view.backgroundColor = .clear
    }

    let controlFactory: MessageSheetControlFactoryProtocol

    public var backgroundInsets: UIEdgeInsets = .zero {
        didSet {
            applyBackgroundViewConstraints()

            setNeedsLayout()
        }
    }

    public var contentInsets: UIEdgeInsets = .zero {
        didSet {
            applyLayoutConstraints()

            setNeedsLayout()
        }
    }

    public var afterGraphicsSpacing: CGFloat = 8 {
        didSet {
            if let graphicsView {
                titleDetailsStackView?.setCustomSpacing(afterGraphicsSpacing, after: graphicsView)
            }
        }
    }

    public var afterTitleSpacing: CGFloat = 8 {
        didSet {
            titleDetailsStackView?.setCustomSpacing(afterTitleSpacing, after: titleLabel)
        }
    }

    public var afterDetailsSpacing: CGFloat = 16 {
        didSet {
            titleDetailsStackView?.layoutMargins.bottom = afterDetailsSpacing
        }
    }

    public var buttonsSpacing: CGFloat = 8 {
        didSet {
            buttonsStackView?.spacing = buttonsSpacing
        }
    }

    public var buttonsAxis: MessageSheetAxis = .horizontal {
        didSet {
            applyButtonsAxis()
        }
    }

    public var actionHeight: CGFloat = 52 {
        didSet {
            mainActionButton?.snp.updateConstraints { make in
                make.height.equalTo(actionHeight)
            }

            secondaryActionButton?.snp.updateConstraints { make in
                make.height.equalTo(actionHeight)
            }

            tertiaryActionButton?.snp.updateConstraints { make in
                make.height.equalTo(actionHeight)
            }

            setNeedsLayout()
        }
    }

    public var buttonsOrder: MessageSheetButtonsOrder = .secondaryMain {
        didSet {
            applyButtonsOrders()
        }
    }

    init(controlFactory: MessageSheetControlFactoryProtocol) {
        self.controlFactory = controlFactory

        super.init(frame: .zero)

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func applyLayoutConstraints() {
        applyInsets(contentInsets, view: layoutView)
    }

    private func applyBackgroundViewConstraints() {
        applyInsets(backgroundInsets, view: backgroundView)
    }

    private func applyInsets(_ insets: UIEdgeInsets, view: UIView) {
        view.snp.remakeConstraints { make in
            make.top.equalToSuperview().inset(insets.top)
            make.leading.equalToSuperview().inset(insets.left)
            make.trailing.equalToSuperview().inset(insets.right)
            make.bottom.equalToSuperview().inset(insets.bottom)
        }
    }

    private func applyButtonsAxis() {
        switch buttonsAxis {
        case .horizontal:
            buttonsStackView?.axis = .horizontal
        case .vertical:
            buttonsStackView?.axis = .vertical
        }
    }

    private func applyButtonsOrders() {
        guard let buttonsStackView else {
            return
        }

        var buttons =
            switch buttonsOrder {
            case .mainSecondary:
                [mainActionButton, secondaryActionButton].compactMap { $0 }
            case .secondaryMain:
                [secondaryActionButton, mainActionButton].compactMap { $0 }
            }

        if let tertiaryActionButton {
            buttons.append(tertiaryActionButton)
        }

        buttons.forEach { $0.removeFromSuperview() }
        buttons.forEach { buttonsStackView.addArrangedSubview($0) }
    }

    private func setupButtonsStackViewIfNeeded() {
        guard buttonsStackView == nil else {
            return
        }

        let stackView = UIStackView()
        stackView.distribution = .fillEqually
        stackView.alignment = .fill
        stackView.spacing = buttonsSpacing

        layoutView.addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.leading.trailing.bottom.equalToSuperview()

            if let titleDetailsStackView {
                make.top.equalTo(titleDetailsStackView.snp.bottom)
            }
        }

        buttonsStackView = stackView

        applyButtonsAxis()
    }

    func setupGraphicsView() {
        let imageView = UIImageView()
        imageView.contentMode = .center

        titleDetailsStackView?.insertArranged(view: imageView, before: titleLabel)
        titleDetailsStackView?.setCustomSpacing(afterGraphicsSpacing, after: imageView)

        graphicsView = imageView
    }

    func setupMainActionButton() {
        setupButtonsStackViewIfNeeded()

        let button = controlFactory.createMain()

        mainActionButton = button

        applyButtonsOrders()

        button.snp.makeConstraints { make in
            make.height.equalTo(actionHeight)
        }
    }

    func setupSecondaryActionButton() {
        setupButtonsStackViewIfNeeded()

        let button = controlFactory.createSecondary()

        secondaryActionButton = button

        applyButtonsOrders()

        button.snp.makeConstraints { make in
            make.height.equalTo(actionHeight)
        }
    }

    func setupTertiaryActionButton() {
        setupButtonsStackViewIfNeeded()

        let button = controlFactory.createTertiary()

        tertiaryActionButton = button

        applyButtonsOrders()

        button.snp.makeConstraints { make in
            make.height.equalTo(actionHeight)
        }
    }

    private func setupLayout() {
        addSubview(backgroundView)
        applyBackgroundViewConstraints()

        backgroundView.addSubview(layoutView)
        applyLayoutConstraints()

        let stackView = UIView.vStack([titleLabel, detailsLabel])
        titleDetailsStackView = stackView
        stackView.isLayoutMarginsRelativeArrangement = true
        stackView.layoutMargins = UIEdgeInsets(top: 0, left: 0, bottom: afterDetailsSpacing, right: 0)

        titleDetailsStackView?.setCustomSpacing(afterTitleSpacing, after: titleLabel)

        layoutView.addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.top.equalToSuperview()
            make.leading.trailing.equalToSuperview().inset(8)
        }
    }
}

extension TitleDetailsSheetViewLayout: MessageSheetStyleAcceptable {}
