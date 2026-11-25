import UIKit
import DesignSystem
internal import SnapKit

final class ChatTransferReclaimView: UIView {
    private let reclaimButton = DSButtonView(
        "",
        style: .secondaryInverted,
        shape: .rounded,
        size: .medium,
        expands: true,
        horizontalPadding: DSSpacings.extraLarge
    )

    override init(frame: CGRect) {
        super.init(frame: frame)

        addSubview(reclaimButton)
        reclaimButton.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func bind(title: String, action: ChatTransferMessageConfiguration.ReclaimAction?) {
        isHidden = action == nil
        reclaimButton.title = title
        reclaimButton.onTap = action?.onReclaim
    }
}
