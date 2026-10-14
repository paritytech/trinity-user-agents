import UIKit
import DesignSystem

final class DSTabBarItemView: UIView {
    /// Action items never receive the pill, so an open panel is signalled by tinting the item.
    var isActive: Bool = false {
        didSet {
            guard isActive != oldValue else {
                return
            }
            applyTint()
        }
    }

    private let isSelectedAppearance: Bool
    private let iconView = UIImageView()
    private let tabsGlyphView = DSTabBarTabsGlyphView()
    private let titleLabel = UILabel()
    private let badgeView = UIView()
    private var glassBackground: DSGlassBackgroundView?

    init(isSelectedAppearance: Bool) {
        self.isSelectedAppearance = isSelectedAppearance

        super.init(frame: .zero)

        setupSubviews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func apply(_ item: DSTabBarItem) {
        switch item.content {
        case let .icon(image):
            iconView.image = image.withRenderingMode(.alwaysTemplate)
            iconView.isHidden = false
            tabsGlyphView.isHidden = true
        case let .tabsGlyph(count):
            tabsGlyphView.count = count
            tabsGlyphView.isHidden = false
            iconView.isHidden = true
        }

        titleLabel.text = item.title
        titleLabel.isHidden = item.title == nil

        badgeView.isHidden = item.badge == nil
        badgeView.backgroundColor = item.badge?.color

        isAccessibilityElement = false
        accessibilityIdentifier = item.accessibilityIdentifier

        if item.showsGlassBackground, !isSelectedAppearance {
            if glassBackground == nil {
                let background = DSGlassBackgroundView(shape: .capsule, style: .clear)
                background.isUserInteractionEnabled = false
                insertSubview(background, at: 0)
                glassBackground = background
            }
            glassBackground?.isHidden = false
        } else {
            glassBackground?.isHidden = true
        }

        applyTint()
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        glassBackground?.frame = bounds

        let iconSize = DSTabBarMetrics.iconSize
        let iconX = ((bounds.width - iconSize) / 2).rounded()
        let iconY = titleLabel.isHidden
            ? ((bounds.height - iconSize) / 2).rounded()
            : DSTabBarMetrics.itemTopPadding

        let glyphFrame = CGRect(x: iconX, y: iconY, width: iconSize, height: iconSize)
        iconView.frame = glyphFrame
        tabsGlyphView.frame = glyphFrame

        let titleHeight = titleLabel.font.lineHeight.rounded(.up)
        titleLabel.frame = CGRect(
            x: 0,
            y: bounds.height - DSTabBarMetrics.itemBottomPadding - titleHeight,
            width: bounds.width,
            height: titleHeight
        )

        let diameter = DSTabBarMetrics.badgeDiameter
        badgeView.frame = CGRect(
            x: iconView.frame.maxX - diameter + 1.3,
            y: iconView.frame.minY,
            width: diameter,
            height: diameter
        )
        badgeView.layer.cornerRadius = diameter / 2
    }
}

private extension DSTabBarItemView {
    var tint: UIColor {
        isSelectedAppearance || isActive ? .fgPrimary : .fgSecondary
    }

    func setupSubviews() {
        isUserInteractionEnabled = false

        iconView.contentMode = .scaleAspectFit
        addSubview(iconView)

        tabsGlyphView.isHidden = true
        addSubview(tabsGlyphView)

        titleLabel.font = .labelSmallEmphasized
        titleLabel.textAlignment = .center
        addSubview(titleLabel)

        badgeView.isHidden = true
        addSubview(badgeView)

        registerForTraitChanges([DSThemeTrait.self]) { (view: DSTabBarItemView, _) in
            view.applyTint()
            view.tabsGlyphView.refreshColorsForTraitChange()
        }

        applyTint()
    }

    func applyTint() {
        let color = tint
        iconView.tintColor = color
        titleLabel.textColor = color
        tabsGlyphView.glyphColor = color
        glassBackground?.tint = isActive ? .bgActionPrimary.withAlphaComponent(0.2) : .bgSurfaceMain
    }
}
