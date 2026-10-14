import UIKit
import DesignSystem
internal import SnapKit
internal import UIKit_iOS

public final class ShareViewLayout: BottomSheetBaseLayout {
    public struct ViewModel {
        public let contacts: [IdentifiableContentConfiguration<String, SelectableContactConfiguration>]
        public let isShareVisible: Bool
        public let isLoading: Bool

        public init(
            contacts: [IdentifiableContentConfiguration<String, SelectableContactConfiguration>],
            isShareVisible: Bool,
            isLoading: Bool
        ) {
            self.contacts = contacts
            self.isShareVisible = isShareVisible
            self.isLoading = isLoading
        }
    }

    public var didTapShare: (() -> Void)?
    public var didTapCancel: (() -> Void)?
    public var didTapTrailingHeaderIcon: (() -> Void)?

    private let headerTitleLabel: Label = .create { label in
        label.typography = .titleMedium
        label.textColor = UIColor(resource: .textAndIconsPrimaryDark)
        label.textAlignment = .center
        label.text = String(localized: .shareTitle)
    }

    private let headerTrailingIconView: UIImageView = .create { view in
        view.contentMode = .scaleAspectFit
        view.tintColor = UIColor(resource: .textAndIconsSecondary)
        view.isUserInteractionEnabled = true
        view.image = UIImage(resource: .iconShare).withRenderingMode(.alwaysTemplate)
    }

    public let contactsGridView = ShareContactsGridView()

    public let shareButton = DSLoadableButtonView(
        contentView: DSButtonView(
            String(localized: .shareButton),
            style: .primary,
            size: .large,
            expands: true
        )
    )

    private let cancelButton = DSButtonView(
        String(localized: .shareCancelButton),
        style: .ghost,
        size: .medium,
        expands: true
    )

    public func bind(viewModel: ViewModel) {
        shareButton.setHidden(!viewModel.isShareVisible)
        shareButton.alpha = shareButton.isHidden ? 0 : 1

        cancelButton.isUserInteractionEnabled = !viewModel.isLoading

        if viewModel.isLoading {
            shareButton.startLoading()
        } else {
            shareButton.stopLoading()
        }

        contactsGridView.bind(contacts: viewModel.contacts)
    }

    override func setupLayout() {
        super.setupLayout()

        contentView.addSubview(headerTitleLabel)
        headerTitleLabel.snp.makeConstraints {
            $0.top.equalToSuperview()
            $0.centerX.equalToSuperview()
        }

        contentView.addSubview(headerTrailingIconView)
        headerTrailingIconView.snp.makeConstraints {
            $0.size.equalTo(24)
            $0.centerY.equalTo(headerTitleLabel)
            $0.trailing.equalToSuperview()
        }

        contentView.addSubview(contactsGridView)
        contactsGridView.snp.makeConstraints {
            $0.top.equalTo(headerTitleLabel.snp.bottom).offset(16)
            $0.leading.trailing.equalToSuperview()
            $0.width.equalTo(contactsGridView.snp.height).multipliedBy(4.0 / 3.0)
        }

        let bottomStack = UIStackView(arrangedSubviews: [shareButton, cancelButton])
        bottomStack.axis = .vertical
        bottomStack.spacing = 8

        contentView.addSubview(bottomStack)
        bottomStack.snp.makeConstraints {
            $0.top.equalTo(contactsGridView.snp.bottom).offset(16)
            $0.leading.trailing.equalToSuperview()
            $0.bottom.equalToSuperview()
        }

        setupHandlers()
    }
}

private extension ShareViewLayout {
    func setupHandlers() {
        let trailingTap = UITapGestureRecognizer(target: self, action: #selector(handleTrailingIconTap))
        headerTrailingIconView.addGestureRecognizer(trailingTap)

        shareButton.contentView.addTarget(self, action: #selector(handleShareTap), for: .touchUpInside)
        cancelButton.addTarget(self, action: #selector(handleCancelTap), for: .touchUpInside)
    }

    @objc func handleTrailingIconTap() {
        didTapTrailingHeaderIcon?()
    }

    @objc func handleShareTap() {
        didTapShare?()
    }

    @objc func handleCancelTap() {
        didTapCancel?()
    }
}
