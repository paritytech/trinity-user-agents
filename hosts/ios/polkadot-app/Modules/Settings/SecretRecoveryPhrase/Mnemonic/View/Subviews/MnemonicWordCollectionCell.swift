import UIKit
import PolkadotUI
import DesignSystem

final class MnemonicWordCollectionCell: CollectionViewContainerCell<TitleValueHorizontalView<Label, Label>> {
    override init(frame: CGRect) {
        super.init(frame: frame)

        configureCell()
        configureLabels()
    }

    // MARK: Public methods

    func bind(model: SecretPhraseMnemonicViewModel.Cell) {
        view.titleView.text = "\(model.index)"
        view.valueView.text = model.text
    }

    // MARK: Private methods

    private func configureCell() {
        if #available(iOS 26.0, *) {
            view.cornerConfiguration = .capsule()
        } else {
            view.layer.cornerRadius = 12
        }

        view.stackView.layoutMargins = .init(horizontal: 8)
        view.stackView.isLayoutMarginsRelativeArrangement = true
        view.backgroundColor = .bgActionTertiary
        view.stackView.alignment = .center
        view.stackView.distribution = .fillProportionally
        view.spacing = 4
        view.titleView.setContentCompressionResistancePriority(.required, for: .horizontal)
        view.titleView.setContentHuggingPriority(.required, for: .horizontal)
    }

    private func configureLabels() {
        view.titleView.textColor = .fgTertiary
        view.titleView.typography = .paragraphLarge
        view.titleView.adjustsFontSizeToFitWidth = true
        view.titleView.minimumScaleFactor = 0.5

        view.valueView.textColor = .fgPrimary
        view.valueView.typography = .paragraphLarge
        view.valueView.adjustsFontSizeToFitWidth = true
        view.valueView.minimumScaleFactor = 0.5

        view.titleView.textAlignment = .left
        view.valueView.textAlignment = .left
    }
}
