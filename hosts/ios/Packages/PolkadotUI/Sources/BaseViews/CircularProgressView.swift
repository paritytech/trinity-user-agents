import UIKit
internal import SnapKit

public final class CircularProgressView: UIView {
    private let overlayView: UIView = .create { view in
        view.backgroundColor = UIColor.black.withAlphaComponent(0.4)
        view.isUserInteractionEnabled = false
    }

    private let progressLayer: CAShapeLayer = {
        let layer = CAShapeLayer()
        layer.fillColor = UIColor.clear.cgColor
        layer.strokeColor = UIColor.white.cgColor
        layer.lineWidth = 3
        layer.lineCap = .round
        layer.strokeStart = 0
        layer.strokeEnd = 0
        return layer
    }()

    private let trackLayer: CAShapeLayer = {
        let layer = CAShapeLayer()
        layer.fillColor = UIColor.clear.cgColor
        layer.strokeColor = UIColor.white.withAlphaComponent(0.3).cgColor
        layer.lineWidth = 3
        layer.lineCap = .round
        return layer
    }()

    private let progressContainerView: UIView = {
        let view = UIView()
        view.backgroundColor = UIColor.black.withAlphaComponent(0.5)
        view.isUserInteractionEnabled = false
        return view
    }()

    private let progressSize: CGFloat = 44

    override public init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        setupProgressViews()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func layoutSubviews() {
        super.layoutSubviews()
        updateProgressPath()
        progressContainerView.layer.cornerRadius = progressSize / 2
    }

    public func updateProgress(_ progress: CGFloat) {
        let clampedProgress = min(max(progress, 0), 1)

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        progressLayer.strokeEnd = clampedProgress
        CATransaction.commit()
    }
}

private extension CircularProgressView {
    func setupProgressViews() {
        addSubview(overlayView)
        overlayView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        addSubview(progressContainerView)
        progressContainerView.clipsToBounds = true
        progressContainerView.snp.makeConstraints { make in
            make.center.equalToSuperview()
            make.width.height.equalTo(progressSize)
        }

        progressContainerView.layer.addSublayer(trackLayer)
        progressContainerView.layer.addSublayer(progressLayer)
    }

    func updateProgressPath() {
        let center = CGPoint(x: progressSize / 2, y: progressSize / 2)
        let radius = (progressSize - progressLayer.lineWidth) / 2
        let path = UIBezierPath(
            arcCenter: center,
            radius: radius,
            startAngle: -.pi / 2,
            endAngle: 3 * .pi / 2,
            clockwise: true
        )

        trackLayer.path = path.cgPath
        progressLayer.path = path.cgPath

        trackLayer.frame = CGRect(x: 0, y: 0, width: progressSize, height: progressSize)
        progressLayer.frame = CGRect(x: 0, y: 0, width: progressSize, height: progressSize)
    }
}
