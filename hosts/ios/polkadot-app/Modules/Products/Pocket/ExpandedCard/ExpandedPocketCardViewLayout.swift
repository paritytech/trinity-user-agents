import SnapKit
import UIKit

final class ExpandedPocketCardViewLayout: UIView {
    let widgetView: WidgetView

    /// Holds the face and the page together, so the card flight moves both as one.
    let stageView = UIView()
    let faceSlot = UIView()

    var isFaceCovered = false

    init(widgetView: WidgetView) {
        self.widgetView = widgetView
        super.init(frame: .zero)

        backgroundColor = .bgSurfaceMain
        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    /// The web view is on top and cannot tell where the page painted nothing, so an uncovered face
    /// takes the touches over it.
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        if !isFaceCovered, let faceView = faceSlot.hitTest(convert(point, to: faceSlot), with: event) {
            return faceView
        }

        return super.hitTest(point, with: event)
    }

    func embedFace(_ faceView: UIView) {
        faceSlot.addSubview(faceView)
        faceView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    func followPage(offset: CGFloat) {
        faceSlot.transform = CGAffineTransform(translationX: 0, y: -offset)
    }

    func bringFaceToFront() {
        stageView.bringSubviewToFront(faceSlot)
    }

    func sendFaceBehindPage() {
        stageView.sendSubviewToBack(faceSlot)
    }
}

private extension ExpandedPocketCardViewLayout {
    func setupLayout() {
        addSubview(stageView)
        stageView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        stageView.addSubview(faceSlot)
        faceSlot.snp.makeConstraints { make in
            make.top.equalTo(safeAreaLayoutGuide)
            make.leading.trailing.equalToSuperview()
            make.height.equalTo(PocketOpenedCardView.height)
        }

        stageView.addSubview(widgetView)
        widgetView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}
