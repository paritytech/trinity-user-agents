import UIKit
public import UIKit_iOS

/// Class to be used to wrap given `WrappedView` with a view that has defined background with either equal `spacing` or
/// precisly set `insets`
/// `style: Style` can be used to adjust for background styles which should reflect agreed designs
/// `mode: Mode` can be used to adjust how `WrappedView` is positioned within this view
open class GenericBackgroundView<WrappedView: UIView>: RoundedView {
    public enum Style {
        case capsule141414
        case capsuleFill6
        case capsuleFill12
        case slightlyRoundedFill6
        case roundedFill6
        case largeRoundedFill6
        case roundedFill12
        case roundedLight
        case roundedLargeLight
        case circleBlack40
        case circleFill6
        case circleFill12
        case circleFill18
        case roundedFill30
        case roundedSecondary
        case roundedTertiary
        case roundedTertiary6
        case blackIcon
    }

    public enum Mode {
        case insets
        case centered
    }

    public let wrappedView: WrappedView

    public var mode: Mode = .insets {
        didSet {
            applyLayout()
        }
    }

    public var style: Style = .roundedFill6 {
        didSet {
            applyStyle()
        }
    }

    /// Used to set equal spacing around `wrappedView` within bounds of `GenericBackgroundView`
    public var spacing: CGFloat = 0 {
        didSet {
            insets = .init(top: spacing, left: spacing, bottom: spacing, right: spacing)
            applyLayout()
        }
    }

    /// Used to setup exact insets for `wrappedView` within bounds of `GenericBackgroundView`
    public var insets: UIEdgeInsets = .zero {
        didSet {
            applyLayout()
        }
    }

    public init(wrappedView: WrappedView = WrappedView()) {
        self.wrappedView = wrappedView
        super.init(frame: .zero)

        setupLayout()
        applyStyle()
    }

    override public init(frame: CGRect) {
        wrappedView = WrappedView()

        super.init(frame: frame)

        setupLayout()
        applyStyle()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension GenericBackgroundView {
    func applyStyle() {
        switch style {
        case .capsule141414:
            applyCapsule141414()
        case .capsuleFill6:
            applyCapsuleFill6()
        case .slightlyRoundedFill6:
            applySlightlyRoundedFill6()
        case .roundedFill6:
            applyRoundedFill6()
        case .largeRoundedFill6:
            applyLargeRoundedFill6()
        case .roundedFill12:
            applyRoundedFill12()
        case .roundedLight:
            applyRoundedLight()
        case .roundedLargeLight:
            applyRoundedLargeLight()
        case .circleBlack40:
            applyBlackCircle(40)
        case .circleFill6:
            applyFill6Circle(40)
        case .circleFill12:
            applyBackgroundStyle(UIColor(resource: .fill12), cornerRadius: 40)
        case .capsuleFill12:
            applyBackgroundStyle(UIColor(resource: .fill12), cornerRadius: 36)
        case .circleFill18:
            applyFill18Circle(40)
        case .roundedFill30:
            applyFill30Rounded()
        case .roundedTertiary:
            applyRoundedTertiary()
        case .blackIcon:
            applyBlackIcon()
        case .roundedTertiary6:
            applyRoundedTertiary6()
        case .roundedSecondary:
            applyRounded16Secondary()
        }
    }

    func applyLayout() {
        wrappedView.removeFromSuperview()
        setupLayout()
    }

    func setupLayout() {
        addSubview(wrappedView)
        switch mode {
        case .insets:
            wrappedView.snp.makeConstraints { make in
                make.leading.equalToSuperview().inset(insets.left)
                make.trailing.equalToSuperview().inset(insets.right)
                make.top.equalToSuperview().inset(insets.top)
                make.bottom.equalToSuperview().inset(insets.bottom)
            }
        case .centered:
            wrappedView.snp.makeConstraints { make in
                make.center.equalToSuperview()
            }
        }
    }
}
