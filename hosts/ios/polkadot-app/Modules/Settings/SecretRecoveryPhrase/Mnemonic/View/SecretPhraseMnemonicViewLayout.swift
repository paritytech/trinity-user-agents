import UIKit
import SnapKit
import UIKit_iOS
import ExternalAccessibility
import PolkadotUI
import DesignSystem

protocol SecretPhraseMnemonicViewDelegate: AnyObject {
    func didTapCopyMnemonic(_ mnemonic: String)
}

final class SecretPhraseMnemonicViewLayout: UIView, AdaptiveDesignable {
    // MARK: Properties

    weak var delegate: SecretPhraseMnemonicViewDelegate?

    let mnemonicView = SecretPhraseMnemonicWordsView()

    var titleLabel: UILabel {
        hintView.fView
    }

    var subtitleLabel: UILabel {
        hintView.sView
    }

    var onViewMnemonic: (() -> Void)?

    private let contentStackView: UIStackView = .create { view in
        view.axis = .vertical
        view.alignment = .center
    }

    private let hintView: GenericPairValueView<Label, Label> = .create { view in
        view.stackView.layoutMargins = .init(horizontal: 32)
        view.stackView.isLayoutMarginsRelativeArrangement = true
        view.setVerticalAndSpacing(8)
        view.stackView.distribution = .fillProportionally

        view.fView.typography = .headlineSmall
        view.fView.textColor = .fgPrimary
        view.fView.textAlignment = .center

        view.sView.typography = .paragraphLarge
        view.sView.textColor = .fgPrimary
        view.sView.numberOfLines = 4
        view.sView.textAlignment = .center
    }

    private let copyButton: DSButtonView = {
        let button = DSButtonView(String(localized: .copyToClipboard), expands: true)
        button.isHidden = true
        return button
    }()

    // MARK: Initial methods

    override init(frame: CGRect) {
        super.init(frame: frame)

        configureView()
        configureActions()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Public methods

    func hideData() {
        mnemonicView.currentType = .hidden
    }

    func bind(viewModel: SecretPhraseMnemonicViewModel) {
        mnemonicView.bind(viewModel: viewModel)
        mnemonicView.snp.remakeConstraints {
            $0.height.equalTo(mnemonicView.intrinsicContentSize.height)
        }
    }
}

private extension SecretPhraseMnemonicViewLayout {
    var layoutInsets: UIEdgeInsets {
        get {
            contentStackView.layoutMargins
        }
        set {
            contentStackView.layoutMargins = newValue
        }
    }

    // MARK: Private methods

    private func addArrangedSubview(_ view: UIView, spacingAfter: CGFloat) {
        contentStackView.addArrangedSubview(view)
        contentStackView.setCustomSpacing(spacingAfter, after: view)
    }

    private func configureView() {
        addSubview(contentStackView)
        contentStackView.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide.snp.top)
            make.leading.trailing.equalToSuperview()
            make.bottom.lessThanOrEqualToSuperview()
        }

        layoutInsets = UIEdgeInsets(
            top: Constants.infoViewTopOffset,
            left: Constants.defaultOffset,
            bottom: .zero,
            right: Constants.defaultOffset
        )

        addArrangedSubview(hintView, spacingAfter: Constants.verticalOffset * 2)
        addArrangedSubview(mnemonicView, spacingAfter: Constants.defaultOffset)

        mnemonicView.delegate = self
        mnemonicView.snp.makeConstraints {
            $0.height.equalTo(mnemonicView.intrinsicContentSize.height)
        }

        addSubview(copyButton)
        copyButton.snp.makeConstraints {
            $0.top.equalTo(mnemonicView.snp.bottom).offset(Constants.defaultOffset)
            $0.centerX.equalTo(mnemonicView.snp.centerX)
            $0.width.equalTo(Constants.copyActionWidth)
        }
    }

    private func configureActions() {
        copyButton.onTap = { [weak self] in
            guard let self else { return }
            delegate?.didTapCopyMnemonic(mnemonicView.fetchMnemonic())
        }

        let tap = UITapGestureRecognizer()
        tap.addTarget(self, action: #selector(onMnemonic))
        mnemonicView.hintView.addGestureRecognizer(tap)
    }

    private func copyButtonAnimation(for type: SecretPhraseMnemonicWordsView.ViewState) {
        let fadeAnimator = FadeAnimator(
            from: type == .hidden ? 1 : .zero,
            to: type == .hidden ? .zero : 1
        )
        fadeAnimator.animate(view: copyButton, completionBlock: nil)
    }

    @objc
    private func onMnemonic() {
        onViewMnemonic?()
    }
}

// MARK: - SecretPhraseMnemonicWordsDelegate

extension SecretPhraseMnemonicViewLayout: SecretPhraseMnemonicWordsDelegate {
    func didTapMnemonic(_ type: SecretPhraseMnemonicWordsView.ViewState) {
        copyButton.isHidden = (type == .hidden)
        copyButtonAnimation(for: type)
    }
}

// MARK: - Constants

private enum Constants {
    static let infoViewTopOffset: CGFloat = 8
    static let defaultOffset: CGFloat = 16
    static let verticalOffset: CGFloat = 24
    static let copyActionWidth: CGFloat = 204
}

// MARK: - AccessibilityBound

extension SecretPhraseMnemonicViewLayout: AccessibilityBound {
    var accessibilityBindings: [AccessibilityBinding] {
        [
            .init(mnemonicView.hintView, AccessibilityID.Settings.tapToRevealButton),
            .init(copyButton, AccessibilityID.Settings.copyRecoveryPhraseButton)
        ]
    }
}
