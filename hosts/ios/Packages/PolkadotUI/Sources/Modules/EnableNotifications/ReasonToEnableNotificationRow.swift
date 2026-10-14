import UIKit
import DesignSystem

public final class ReasonToEnableNotificationRow: GenericBackgroundView<GenericPairValueView<
    GenericBackgroundView<UIImageView>,
    Label
>> {
    public var pairView: GenericPairValueView<GenericBackgroundView<UIImageView>, Label> { wrappedView }
    public var imageViewContainer: GenericBackgroundView<UIImageView> { wrappedView.fView }
    public var imageView: UIImageView { imageViewContainer.wrappedView }
    public var detailsLabel: Label { wrappedView.sView }

    convenience init(viewModel: ViewModel) {
        self.init(frame: .zero)
        bind(viewModel: viewModel)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        setupLayout()
    }
}

private extension ReasonToEnableNotificationRow {
    func setupLayout() {
        pairView.setHorizontalAndSpacing(12)
        pairView.stackView.isLayoutMarginsRelativeArrangement = true
        pairView.stackView.layoutMargins = .init(top: 8, left: 8, bottom: 8, right: 8)

        style = .capsule141414

        detailsLabel.typography = .titleMedium
        detailsLabel.textColor = UIColor(resource: .textAndIconsPrimaryDark)

        imageViewContainer.insets = UIEdgeInsets(top: 10, left: 10, bottom: 10, right: 10)
        imageViewContainer.backgroundColor = UIColor(resource: .white8)
        imageViewContainer.layer.cornerRadius = 48 / 2
        imageViewContainer.snp.makeConstraints {
            $0.width.height.equalTo(48)
        }

        imageView.contentMode = .scaleAspectFill
    }
}

public extension ReasonToEnableNotificationRow {
    struct ViewModel {
        let image: UIImage
        let details: String

        public init(image: UIImage, details: String) {
            self.image = image
            self.details = details
        }
    }

    func bind(viewModel: ViewModel) {
        imageView.image = viewModel.image
        detailsLabel.text = viewModel.details
    }
}
