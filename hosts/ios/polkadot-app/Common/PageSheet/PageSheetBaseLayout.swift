import UIKit
import UIKit_iOS
import PolkadotUI

class PageSheetBaseLayout: UIView {
    let backgroundView: RoundedView = .create { view in
        view.applyBackgroundStyle(
            .bgSurfaceContainer,
            cornerRadius: BottomSheetStyleConstants.cornerRadius
        )
    }

    let indicatorView: RoundedView = .create { view in
        view.cornerRadius = PageSheetStyleConstants.indicatorSize.height / 2
        view.fillColor = .color3C3C43
        view.shadowOpacity = 0.0
    }

    let contentView: ScrollableContainerLayoutView = .create { view in
        view.backgroundColor = .clear
        view.containerView.scrollView.showsVerticalScrollIndicator = false
    }

    override init(frame: CGRect) {
        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setupLayout() {
        addSubview(backgroundView)
        backgroundView.addSubview(indicatorView)
        backgroundView.addSubview(contentView)

        backgroundView.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(PageSheetStyleConstants.backgroundInsets.left)
            make.trailing.equalToSuperview().inset(PageSheetStyleConstants.backgroundInsets.right)
            make.top.equalToSuperview().inset(PageSheetStyleConstants.backgroundInsets.top)
            make.bottom.equalToSuperview().inset(PageSheetStyleConstants.backgroundInsets.bottom)
        }
        indicatorView.snp.makeConstraints { make in
            make.size.equalTo(PageSheetStyleConstants.indicatorSize)
            make.centerX.equalToSuperview()
            make.top.equalToSuperview().inset(PageSheetStyleConstants.indicatorInsets.top)
            make.bottom.equalTo(contentView.snp.top)
                .inset(-(PageSheetStyleConstants.indicatorInsets.bottom + PageSheetStyleConstants.contentInsets.top))
        }
        contentView.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(PageSheetStyleConstants.contentInsets.left)
            make.trailing.equalToSuperview().inset(PageSheetStyleConstants.contentInsets.right)
            make.bottom.equalToSuperview().inset(PageSheetStyleConstants.contentInsets.bottom)
        }
    }
}
