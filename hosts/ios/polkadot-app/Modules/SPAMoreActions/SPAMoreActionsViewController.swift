import SnapKit
import UIKit
import UIKit_iOS

final class SPAMoreActionsViewController: UIViewController, SPAMoreActionsViewProtocol {
    let presenter: SPAMoreActionsPresenterProtocol

    init(presenter: SPAMoreActionsPresenterProtocol) {
        self.presenter = presenter
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        setupLayout()
    }
}

// MARK: - Layout

private extension SPAMoreActionsViewController {
    func setupLayout() {
        view.backgroundColor = .clear

        let backgroundView = RoundedView()
        backgroundView.applyBackgroundStyle(
            .bgSurfaceContainer,
            cornerRadius: BottomSheetStyleConstants.cornerRadius
        )
        view.addSubview(backgroundView)
        backgroundView.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(BottomSheetStyleConstants.backgroundInsets.left)
            make.trailing.equalToSuperview().inset(BottomSheetStyleConstants.backgroundInsets.right)
            make.top.equalToSuperview()
            make.bottom.equalToSuperview().inset(BottomSheetStyleConstants.backgroundInsets.bottom)
        }

        let contentStack = UIStackView()
        contentStack.axis = .vertical
        contentStack.spacing = 0

        backgroundView.addSubview(contentStack)
        contentStack.snp.makeConstraints { make in
            make.top.equalToSuperview().inset(24)
            make.leading.trailing.equalToSuperview()
            make.bottom.equalToSuperview().inset(16)
        }

        for (index, action) in presenter.actions.enumerated() {
            let row = createActionRow(action: action, index: index)
            contentStack.addArrangedSubview(row)
        }

        let separator = UIView()
        separator.backgroundColor = .clear
        separator.snp.makeConstraints { make in
            make.height.equalTo(8)
        }
        contentStack.addArrangedSubview(separator)

        let closeButton = createCloseButton()
        contentStack.addArrangedSubview(closeButton)
    }

    func createActionRow(action: SPAMoreAction, index: Int) -> UIView {
        let container = UIButton(type: .system)
        container.isEnabled = action.isEnabled
        container.tag = index

        let iconView = UIImageView()
        iconView.image = action.icon?.withRenderingMode(.alwaysTemplate)
        iconView.tintColor = action.isEnabled ? .fgSecondary : .fgDisabled
        iconView.contentMode = .scaleAspectFit

        let label = UILabel()
        label.text = action.title
        label.font = .semibold16
        label.textColor = action.isEnabled ? .fgSecondary : .fgDisabled

        container.addSubview(iconView)
        container.addSubview(label)

        iconView.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(24)
            make.centerY.equalToSuperview()
            make.size.equalTo(24)
        }

        label.snp.makeConstraints { make in
            make.leading.equalTo(iconView.snp.trailing).offset(16)
            make.trailing.equalToSuperview().inset(24)
            make.centerY.equalToSuperview()
        }

        container.snp.makeConstraints { make in
            make.height.equalTo(48)
        }

        if action.isEnabled {
            container.addAction(UIAction { [weak self] uiAction in
                guard let button = uiAction.sender as? UIButton else { return }
                self?.presenter.didSelectAction(at: button.tag)
            }, for: .touchUpInside)
        }

        return container
    }

    func createCloseButton() -> UIView {
        let button = UIButton(type: .system)
        button.setTitle(presenter.closeTitle, for: .normal)
        button.setTitleColor(.fgTertiary, for: .normal)
        button.titleLabel?.font = .semibold16

        button.snp.makeConstraints { make in
            make.height.equalTo(44)
        }

        button.addAction(UIAction { [weak self] _ in
            self?.presenter.didSelectClose()
        }, for: .touchUpInside)

        return button
    }
}
