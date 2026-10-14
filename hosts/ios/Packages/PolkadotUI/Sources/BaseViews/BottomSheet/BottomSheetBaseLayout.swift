import DesignSystem
import UIKit
internal import UIKit_iOS
internal import SnapKit

open class BottomSheetBaseLayout: UIView {
    let backgroundView: RoundedView = .create { view in
        view.applyBackgroundStyle(
            .bgSurfaceContainer,
            cornerRadius: BottomSheetStyleConstants.cornerRadius
        )
    }

    let contentView: UIView = .create { view in
        view.backgroundColor = .clear
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setupLayout() {
        addSubview(backgroundView)
        backgroundView.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(BottomSheetStyleConstants.backgroundInsets.left)
            make.trailing.equalToSuperview().inset(BottomSheetStyleConstants.backgroundInsets.right)
            make.top.equalToSuperview().inset(BottomSheetStyleConstants.backgroundInsets.top)
            make.bottom.equalToSuperview().inset(BottomSheetStyleConstants.backgroundInsets.bottom)
        }

        backgroundView.addSubview(contentView)
        contentView.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(BottomSheetStyleConstants.contentInsets.left)
            make.trailing.equalToSuperview().inset(BottomSheetStyleConstants.contentInsets.right)
            make.top.equalToSuperview().inset(BottomSheetStyleConstants.contentInsets.top)
            make.bottom.equalToSuperview().inset(BottomSheetStyleConstants.contentInsets.bottom)
        }
    }
}
