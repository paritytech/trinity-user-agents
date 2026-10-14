import UIKit
import UIKit_iOS

final class BlurredBackgroundView<WrappedView: UIView>: UIView {
    enum Style {
        case darkRounded
    }

    enum Mode {
        case insets
        case centered
    }

    let wrappedView: WrappedView
    private var visualEffectView: UIVisualEffectView?

    var mode: Mode = .insets {
        didSet {
            applyLayout()
        }
    }

    var style: Style = .darkRounded {
        didSet {
            applyStyle()
        }
    }

    /// Used to set equal spacing around `wrappedView` within bounds of `GenericBackgroundView`
    var spacing: CGFloat = 0 {
        didSet {
            insets = .init(top: spacing, left: spacing, bottom: spacing, right: spacing)
            applyLayout()
        }
    }

    /// Used to setup exact insets for `wrappedView` within bounds of `GenericBackgroundView`
    var insets: UIEdgeInsets = .zero {
        didSet {
            applyLayout()
        }
    }

    init(wrappedView: WrappedView = WrappedView()) {
        self.wrappedView = wrappedView
        super.init(frame: .zero)

        setupLayout()
        applyStyle()
    }

    override init(frame: CGRect) {
        wrappedView = WrappedView()

        super.init(frame: frame)

        setupLayout()
        applyStyle()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

private extension BlurredBackgroundView {
    func applyStyle() {
        visualEffectView?.removeFromSuperview()
        let blurEffectStyle: UIBlurEffect.Style
        switch style {
        case .darkRounded:
            blurEffectStyle = .dark
            layer.cornerRadius = 24
            clipsToBounds = true
        }
        let visualEffectView = UIVisualEffectView(effect: UIBlurEffect(style: blurEffectStyle))
        visualEffectView.clearsContextBeforeDrawing = true
        addSubview(visualEffectView)
        visualEffectView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        self.visualEffectView = visualEffectView
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
