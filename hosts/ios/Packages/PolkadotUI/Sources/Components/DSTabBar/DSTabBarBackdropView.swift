import UIKit
import DesignSystem

/// Full-bleed dimming behind the tab bar chrome while a panel is open.
public final class DSTabBarBackdropView: UIView {
    public private(set) var isOpen = false

    override public init(frame: CGRect) {
        super.init(frame: frame)

        // The chrome's passthrough hit test returns early on any subview hit, which would
        // swallow the outside tap that closes the panel.
        isUserInteractionEnabled = false
        alpha = 0
        applyColors()

        registerForTraitChanges([DSThemeTrait.self]) { (view: DSTabBarBackdropView, _) in
            view.applyColors()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setOpen(_ open: Bool, animator: UIViewPropertyAnimator?) {
        guard open != isOpen else {
            return
        }
        isOpen = open

        let apply = { [self] in
            alpha = open ? 1 : 0
        }

        guard let animator else {
            apply()
            return
        }

        animator.addAnimations(apply)
    }
}

private extension DSTabBarBackdropView {
    func applyColors() {
        backgroundColor = .bgSurfaceOverlay
    }
}
