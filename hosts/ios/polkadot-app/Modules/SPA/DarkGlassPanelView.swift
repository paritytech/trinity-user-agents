import SnapKit
import UIKit

final class DarkGlassPanelView: UIView {
    private let cornerRadius: CGFloat
    private let contentContainer = UIView()

    init(cornerRadius: CGFloat) {
        self.cornerRadius = cornerRadius
        super.init(frame: .zero)

        isUserInteractionEnabled = false
        layer.masksToBounds = false
        layer.shadowColor = UIColor.black.cgColor
        layer.shadowRadius = Constants.shadowRadius
        layer.shadowOpacity = Constants.shadowOpacity
        layer.shadowOffset = CGSize(width: 0, height: 1)

        contentContainer.layer.cornerRadius = cornerRadius
        contentContainer.layer.cornerCurve = .continuous
        contentContainer.clipsToBounds = true
        contentContainer.layer.borderWidth = Constants.rimWidth
        contentContainer.layer.borderColor = UIColor.white.withAlphaComponent(Constants.rimAlpha).cgColor
        addSubview(contentContainer)

        let blurView = UIVisualEffectView(effect: UIBlurEffect(style: .systemUltraThinMaterialDark))
        contentContainer.addSubview(blurView)

        let fillView = UIView()
        fillView.backgroundColor = Constants.fillColor
        contentContainer.addSubview(fillView)

        contentContainer.snp.makeConstraints { make in make.edges.equalToSuperview() }
        blurView.snp.makeConstraints { make in make.edges.equalToSuperview() }
        fillView.snp.makeConstraints { make in make.edges.equalToSuperview() }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        layer.shadowPath = UIBezierPath(roundedRect: bounds, cornerRadius: cornerRadius).cgPath
    }
}

private extension DarkGlassPanelView {
    enum Constants {
        static let fillColor = UIColor(red: 28.0 / 255.0, green: 28.0 / 255.0, blue: 28.0 / 255.0, alpha: 0.85)
        static let rimWidth: CGFloat = 0.8
        static let rimAlpha: CGFloat = 0.15
        static let shadowRadius: CGFloat = 24.0
        static let shadowOpacity: Float = 0.04
    }
}
