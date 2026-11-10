import DesignSystem
import PolkadotUI
import SnapKit
import UIKit

/// The placeholder the Collectibles page shows while its product loads: the shape of the collection, with a sweep of
/// light across it. A load that fails clears the shape and shows the error in its place.
final class CollectiblesSkeletonView: UIView {
    private static let padding: CGFloat = 16
    private static let cardHeight: CGFloat = 260
    private static let cardRadius: CGFloat = 24
    private static let sweepDuration: TimeInterval = 1.35

    private let shape = UIView()
    private let titleBlock = UIView()
    private let subtitleBlock = UIView()
    private let avatar = UIView()
    private let firstCard = UIView()
    private let secondCard = UIView()
    private let sweep = CAGradientLayer()
    private let errorLabel: PolkadotUI.Label = .create { view in
        view.typography = .bodyMedium
        view.textColor = .fgSecondary
        view.numberOfLines = 0
        view.textAlignment = .center
        view.isHidden = true
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        backgroundColor = .bgSurfaceMain
        setupShape()
        setupErrorLabel()
        setupSweep()
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: CollectiblesSkeletonView, _) in
            view.updateSweepColors()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()

        sweep.frame = CGRect(x: 0, y: 0, width: bounds.width / 2, height: bounds.height)
    }

    /// Sweeps light across the placeholder until `stopSweep` or `showFailure`.
    func startSweep() {
        sweep.isHidden = false
        let animation = CABasicAnimation(keyPath: "transform.translation.x")
        animation.fromValue = -sweep.bounds.width
        animation.toValue = bounds.width
        animation.duration = Self.sweepDuration
        animation.repeatCount = .infinity
        animation.timingFunction = CAMediaTimingFunction(name: .linear)
        sweep.add(animation, forKey: "sweep")
    }

    func stopSweep() {
        sweep.removeAnimation(forKey: "sweep")
        sweep.isHidden = true
    }

    /// Stops the sweep, clears the placeholder shape, and shows `message` in its place.
    func showFailure(_ message: String) {
        stopSweep()
        shape.isHidden = true
        errorLabel.text = message
        errorLabel.isHidden = false
    }
}

private extension CollectiblesSkeletonView {
    func setupShape() {
        addSubview(shape)
        shape.accessibilityElementsHidden = true
        shape.addSubview(titleBlock)
        shape.addSubview(subtitleBlock)
        shape.addSubview(avatar)
        shape.addSubview(firstCard)
        shape.addSubview(secondCard)

        shape.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
        titleBlock.snp.makeConstraints { make in
            make.top.equalToSuperview().offset(Self.padding)
            make.centerX.equalToSuperview()
            make.width.equalTo(136)
            make.height.equalTo(16)
        }
        subtitleBlock.snp.makeConstraints { make in
            make.top.equalTo(titleBlock.snp.bottom).offset(8)
            make.centerX.equalToSuperview()
            make.width.equalTo(180)
            make.height.equalTo(10)
        }
        avatar.snp.makeConstraints { make in
            make.top.trailing.equalToSuperview().inset(Self.padding)
            make.size.equalTo(40)
        }
        firstCard.snp.makeConstraints { make in
            make.top.equalTo(subtitleBlock.snp.bottom).offset(24)
            make.leading.trailing.equalToSuperview().inset(Self.padding)
            make.height.equalTo(Self.cardHeight)
        }
        secondCard.snp.makeConstraints { make in
            make.top.equalTo(firstCard.snp.bottom).offset(Self.padding)
            make.leading.trailing.bottom.equalToSuperview().inset(Self.padding)
        }

        [titleBlock, subtitleBlock, avatar].forEach { $0.backgroundColor = .bgSurfaceNested }
        titleBlock.layer.cornerRadius = 8
        subtitleBlock.layer.cornerRadius = 5
        avatar.layer.cornerRadius = 20

        setupCard(firstCard, labelWidth: 80)
        setupCard(secondCard, labelWidth: 116)
    }

    func setupCard(_ card: UIView, labelWidth: CGFloat) {
        let label = UIView()
        let dots = DotGridView()
        label.backgroundColor = .bgSurfaceNested
        label.layer.cornerRadius = 7
        card.addSubview(label)
        card.addSubview(dots)

        card.backgroundColor = .bgSurfaceContainer
        card.layer.cornerRadius = Self.cardRadius
        card.layer.cornerCurve = .continuous

        label.snp.makeConstraints { make in
            make.top.leading.equalToSuperview().inset(Self.padding)
            make.width.equalTo(labelWidth)
            make.height.equalTo(14)
        }
        dots.snp.makeConstraints { make in
            make.top.equalTo(label.snp.bottom).offset(Self.padding)
            make.leading.trailing.bottom.equalToSuperview().inset(Self.padding)
        }
    }

    func setupErrorLabel() {
        addSubview(errorLabel)
        errorLabel.snp.makeConstraints { make in
            make.center.equalToSuperview()
            make.leading.trailing.equalToSuperview().inset(Self.padding)
        }
    }

    func setupSweep() {
        sweep.startPoint = CGPoint(x: 0, y: 0.5)
        sweep.endPoint = CGPoint(x: 1, y: 0.5)
        sweep.isHidden = true
        updateSweepColors()
        layer.addSublayer(sweep)
    }

    func updateSweepColors() {
        let highlight = UIColor.fgPrimary.withAlphaComponent(0.12).cgColor
        sweep.colors = [UIColor.clear.cgColor, highlight, UIColor.clear.cgColor]
    }
}

/// A grid of dots, the placeholder for the artwork of a collectible.
private final class DotGridView: UIView {
    private static let columns = 12
    private static let pitch: CGFloat = 24
    private static let dotRadius: CGFloat = 2.5

    override func draw(_: CGRect) {
        UIColor.fgTertiary.withAlphaComponent(0.25).setFill()
        let rows = Int(bounds.height / Self.pitch)
        for row in 0 ..< rows {
            for column in 0 ..< Self.columns {
                let center = CGPoint(
                    x: CGFloat(column) * Self.pitch + Self.pitch / 2,
                    y: CGFloat(row) * Self.pitch + Self.pitch / 2
                )
                let dot = UIBezierPath(
                    arcCenter: center,
                    radius: Self.dotRadius,
                    startAngle: 0,
                    endAngle: 2 * .pi,
                    clockwise: true
                )
                dot.fill()
            }
        }
    }
}
