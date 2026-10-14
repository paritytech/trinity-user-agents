import DesignSystem
import UIKit
internal import SnapKit
internal import UIKit_iOS

public struct SelectableAvatarConfiguration: Hashable {
    public let avatar: AvatarViewModel
    public let isSelected: Bool

    public init(avatar: AvatarViewModel, isSelected: Bool) {
        self.avatar = avatar
        self.isSelected = isSelected
    }
}

public final class SelectableAvatarView: UIView {
    private enum Constants {
        static let ringStrokeWidth: CGFloat = 2
        static let avatarInsetWhenSelected: CGFloat = 5
        static let badgeSize: CGFloat = 21
        static let badgeStrokeWidth: CGFloat = 2
        static let checkmarkSize: CGFloat = 14
    }

    private let avatarView = DSAvatarView(size: .s48)
    private let avatarMaskLayer = CAShapeLayer()

    private let ringView: UIView = .create { view in
        view.backgroundColor = .clear
        view.layer.borderWidth = Constants.ringStrokeWidth
        view.isUserInteractionEnabled = false
        view.alpha = 0
    }

    private let badgeView: RoundedView = .create { view in
        view.applyBorderStyle(
            .strokeCutout,
            backgroundColor: .bgAccent,
            strokeWidth: Constants.badgeStrokeWidth,
            cornerRadius: Constants.badgeSize / 2
        )
        view.isUserInteractionEnabled = false
        view.alpha = 0
    }

    private let checkmarkImageView: UIImageView = .create { view in
        view.image = UIImage(resource: .check18).withRenderingMode(.alwaysTemplate)
        view.tintColor = .fgStaticWhite
        view.contentMode = .scaleAspectFit
    }

    private var isSelected = false

    public var proposedDimension: CGFloat {
        avatarView.proposedDimension
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)
        avatarView.layer.mask = avatarMaskLayer
        setupViews()
        applyRingColor()
        registerForTraitChanges([DSThemeTrait.self]) { (view: SelectableAvatarView, _) in
            view.applyRingColor()
        }
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func layoutSubviews() {
        super.layoutSubviews()
        let diameter = min(bounds.width, bounds.height)
        ringView.layer.cornerRadius = diameter / 2
        updateAvatarMask()
    }

    public func configure(with config: SelectableAvatarConfiguration) {
        avatarView.viewModel = config.avatar
        isSelected = config.isSelected

        let alpha: CGFloat = isSelected ? 1 : 0
        ringView.alpha = alpha
        badgeView.alpha = alpha

        updateAvatarMask()
    }

    private func applyRingColor() {
        ringView.layer.borderColor = UIColor.bgAccent.resolvedColor(with: traitCollection).cgColor
    }

    private func updateAvatarMask() {
        let inset: CGFloat = isSelected ? Constants.avatarInsetWhenSelected : 0
        let rect = avatarView.bounds.insetBy(dx: inset, dy: inset)
        avatarMaskLayer.frame = avatarView.bounds
        avatarMaskLayer.path = UIBezierPath(ovalIn: rect).cgPath
    }

    private func setupViews() {
        setContentHuggingPriority(.required, for: .vertical)
        setContentHuggingPriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .vertical)
        setContentCompressionResistancePriority(.required, for: .horizontal)

        addSubview(avatarView)
        avatarView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }

        addSubview(ringView)
        ringView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }

        addSubview(badgeView)
        badgeView.snp.makeConstraints {
            $0.size.equalTo(Constants.badgeSize)
            $0.trailing.equalToSuperview().inset(-1)
            $0.bottom.equalToSuperview().inset(-1)
        }

        badgeView.addSubview(checkmarkImageView)
        checkmarkImageView.snp.makeConstraints {
            $0.center.equalToSuperview()
            $0.size.equalTo(Constants.checkmarkSize)
        }
    }
}
