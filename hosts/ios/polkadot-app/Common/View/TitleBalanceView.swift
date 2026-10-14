import UIKit
import UIKit_iOS
import PolkadotUI
import DesignSystem

final class TitleBalanceView: ControlView<UIView, TitleValueHorizontalView<IconLabelView, UILabel>> {
    var titleLabel: UILabel {
        controlContentView.titleView.detailsView
    }

    var accessoryImageView: UIImageView {
        controlContentView.titleView.imageView
    }

    var detailsLabel: UILabel {
        controlContentView.valueView
    }

    var locale: Locale = .current {
        didSet {
            setupLocalization()
        }
    }

    let balanceFormatter = InlineBalanceFormatter()

    var canSelect: Bool {
        get {
            isUserInteractionEnabled
        }

        set {
            isUserInteractionEnabled = newValue

            updateSelectionState()
        }
    }

    var skeletonView: SkrullableView?

    private var isLoading: Bool = false

    convenience init() {
        self.init(frame: .zero)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        configure()
        setupLocalization()
    }

    func bind(viewModel: BalanceViewModelProtocol?) {
        guard let viewModel else {
            startLoadingIfNeeded()
            return
        }
        stopLoadingIfNeeded()
        detailsLabel.attributedText = balanceFormatter.getFormattedString(from: viewModel)
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        updateSkeletonLayoutIfNeeded()
    }

    private func updateSkeletonLayoutIfNeeded() {
        if isLoading {
            updateLoadingState()
            skeletonView?.restartSkrulling()
        }
    }

    private func setupLocalization() {
        titleLabel.text = String(localized: .feeTitle)
    }

    private func configure() {
        preferredHeight = 44
        contentInsets = UIEdgeInsets(top: 12, left: 0, bottom: 12, right: 0)
        changesContentOpacityWhenHighlighted = true

        controlContentView.isUserInteractionEnabled = false

        controlContentView.titleView.makeHorizontal(with: .detailsIcon, spacing: 8)
        controlContentView.usesSpacer = true

        titleLabel.textColor = .fgTertiary
        titleLabel.font = UIFont.bodyMedium

        controlContentView.titleView.iconWidth = 16

        detailsLabel.textColor = .fgPrimary
        detailsLabel.font = UIFont.bodyMedium

        updateSelectionState()
    }

    private func updateSelectionState() {
        accessoryImageView.image = nil // isUserInteractionEnabled ? .info16 : nil
    }
}

extension TitleBalanceView: SkeletonableView {
    var skeletonStyle: SkeletonableViewStyle {
        .init(startColor: .white12, endColor: .white24)
    }

    func createSkeletons(for spaceSize: CGSize) -> [Skeletonable] {
        let size = CGSize(width: 78, height: 20)

        let amountRow = SingleSkeleton.createRow(
            on: self,
            containerView: self,
            spaceSize: spaceSize,
            offset: CGPoint(
                x: spaceSize.width - contentInsets.right - size.width,
                y: (preferredHeight ?? 0.0) / 2 - size.height / 2.0
            ),
            size: size
        )

        return [amountRow]
    }

    var skeletonSuperview: UIView {
        self
    }

    var hidingViews: [UIView] {
        [detailsLabel]
    }

    func didStartSkeleton() {
        isLoading = true
    }

    func didStopSkeleton() {
        isLoading = false
    }
}
