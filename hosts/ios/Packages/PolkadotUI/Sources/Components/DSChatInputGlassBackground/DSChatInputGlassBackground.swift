import DesignSystem
import UIKit
internal import SnapKit

final class DSChatInputGlassBackground: UIView {
    init(cornerRadius: CGFloat, fallbackColor: UIColor, interactive: Bool) {
        super.init(frame: .zero)

        layer.cornerCurve = .continuous

        if #available(iOS 26.0, *) {
            let effect = UIGlassEffect()
            effect.isInteractive = interactive
            let effectView = UIVisualEffectView(effect: effect)
            effectView.isUserInteractionEnabled = false
            effectView.cornerConfiguration = .corners(radius: .fixed(cornerRadius))
            addSubview(effectView)
            effectView.snp.makeConstraints { $0.edges.equalToSuperview() }
        } else {
            backgroundColor = fallbackColor
            layer.cornerRadius = cornerRadius
            clipsToBounds = true
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
