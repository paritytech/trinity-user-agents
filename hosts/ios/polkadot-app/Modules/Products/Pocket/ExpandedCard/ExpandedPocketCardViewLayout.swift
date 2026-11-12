import SnapKit
import UIKit

final class ExpandedPocketCardViewLayout: UIView {
    let widgetView: WidgetView

    init(widgetView: WidgetView) {
        self.widgetView = widgetView
        super.init(frame: .zero)

        backgroundColor = .bgSurfaceMain

        addSubview(widgetView)
        widgetView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}
