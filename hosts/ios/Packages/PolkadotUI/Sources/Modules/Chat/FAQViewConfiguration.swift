import Foundation
import UIKit
import DesignSystem

public struct FAQViewConfiguration: HashableContentConfiguration {
    let actions: [UIAction]
    public init(actions: [UIAction]) {
        self.actions = actions
    }

    public func makeContentView() -> any UIView & UIContentView {
        FAQContentView(configuration: self)
    }

    public func updated(for _: any UIConfigurationState) -> FAQViewConfiguration {
        self
    }
}

final class FAQContentView: UIView, UIContentView {
    typealias Configuration = FAQViewConfiguration

    private var appliedConfiguration: Configuration

    var configuration: UIContentConfiguration {
        get { appliedConfiguration }
        set { apply(newValue) }
    }

    private let contentContainer: UIStackView = .create { view in
        view.spacing = 8
        view.axis = .vertical
        view.alignment = .trailing
    }

    init(configuration: Configuration) {
        appliedConfiguration = configuration
        super.init(frame: .zero)
        setupViews()
        apply(configuration)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupViews() {
        addSubview(contentContainer)
        contentContainer.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }

    private func apply(_ any: UIContentConfiguration) {
        guard let configuration = any as? Configuration else { return }
        appliedConfiguration = configuration

        contentContainer.subviews.forEach { $0.removeFromSuperview() }

        let faqButtons = configuration.actions.map {
            let button = FAQButton()
            button.primaryAction = $0
            button.title = $0.title
            return button
        }

        faqButtons.forEach {
            contentContainer.addArrangedSubview($0)
        }
    }
}

extension FAQContentView {
    private final class FAQButton: UIView {
        private let internalButton: UIButton = .create { button in
            button.titleLabel?.font = UIFont.titleSmall
            button.setTitleColor(.fgSecondary, for: .normal)
            button.contentEdgeInsets = UIEdgeInsets(top: 12, left: 16, bottom: 12, right: 16)
            button.backgroundColor = .clear
        }

        private var maskLayer: CAShapeLayer?
        private var borderLayer: CAShapeLayer?

        var title: String? {
            get {
                internalButton.title(for: .normal)
            }
            set {
                internalButton.setTitle(newValue, for: .normal)
            }
        }

        var primaryAction: UIAction? {
            didSet {
                if let oldAction = oldValue {
                    internalButton.removeAction(oldAction, for: .touchUpInside)
                }
                if let newAction = primaryAction {
                    internalButton.addAction(newAction, for: .touchUpInside)
                }
            }
        }

        private let cornerRadius: CGFloat = 12
        private let bottomRightRadius: CGFloat = 4

        var borderWidth: CGFloat = 1.0 {
            didSet {
                setNeedsLayout()
            }
        }

        var borderColor: UIColor = .strokePrimary {
            didSet {
                setNeedsLayout()
            }
        }

        override var backgroundColor: UIColor? {
            didSet {
                guard let color = backgroundColor else {
                    layer.backgroundColor = nil
                    return
                }
                layer.backgroundColor = color.cgColor
            }
        }

        // MARK: - Initialization

        override init(frame: CGRect) {
            super.init(frame: frame)
            setupView()
        }

        required init?(coder: NSCoder) {
            super.init(coder: coder)
            setupView()
        }

        // MARK: - Setup

        private func setupView() {
            maskLayer = CAShapeLayer()
            layer.mask = maskLayer

            borderLayer = CAShapeLayer()
            borderLayer?.fillColor = UIColor.clear.cgColor
            layer.addSublayer(borderLayer!)

            if backgroundColor == nil {
                backgroundColor = .bgSurfaceContainer
            }

            addSubview(internalButton)
            internalButton.snp.makeConstraints { make in
                make.edges.equalToSuperview()
            }
        }

        // MARK: - Layout

        override func layoutSubviews() {
            super.layoutSubviews()

            // Create the custom path for both the mask and the border
            let path = UIBezierPath(
                roundedRect: bounds,
                cornerSpecs: [
                    (.layerMaxXMaxYCorner, bottomRightRadius),
                    (.layerMaxXMinYCorner, cornerRadius),
                    (.layerMinXMaxYCorner, cornerRadius),
                    (.layerMinXMinYCorner, cornerRadius)
                ]
            )

            maskLayer?.path = path.cgPath

            borderLayer?.path = path.cgPath
            borderLayer?.lineWidth = borderWidth
            borderLayer?.strokeColor = borderColor.cgColor
        }
    }
}
