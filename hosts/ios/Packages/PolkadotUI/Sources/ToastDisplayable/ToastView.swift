import Foundation
import DesignSystem
internal import UIKit_iOS
import UIKit

public final class ToastView: GenericBorderedView<IconDetailsGenericView<Label>> {
    public enum ToastType {
        case success
        case error
    }

    init(message: String, type: ToastType) {
        super.init(frame: .zero)

        contentInsets = .init(top: 16, left: 24, bottom: 16, right: 24)
        contentView.mode = .iconDetails
        backgroundView.applyBorderStyle(
            .strokePrimary,
            backgroundColor: type.backgroundColor,
            cornerRadius: 50
        )
        contentView.spacing = 12
        contentView.imageView.image = type.image
        contentView.detailsView.numberOfLines = 3
        contentView.detailsView.adjustsFontSizeToFitWidth = true
        contentView.detailsView.minimumScaleFactor = 0.75
        contentView.detailsView.text = message
        contentView.detailsView.typography = .titleMedium
        contentView.detailsView.textColor = type.textColor
    }

    func show(in view: UIView, duration: TimeInterval = 2.0) {
        view.addSubview(self)

        snp.makeConstraints { make in
            make.centerX.equalToSuperview()
            make.bottom.equalTo(view.safeAreaLayoutGuide.snp.bottom).inset(20)
            make.leading.equalToSuperview().offset(UIConstants.horizontalInsetMedium)
        }

        layoutIfNeeded()

        let offset = frame.height > 0 ? 2 * frame.height : 100
        transform = CGAffineTransform(translationX: 0, y: offset)
        UIView.animate(springDuration: 0.3, bounce: 0.4) {
            self.transform = .identity
        }

        ToastView.cancelPreviousPerformRequests(
            withTarget: self,
            selector: #selector(hide),
            object: nil
        )
        perform(#selector(hide), with: nil, afterDelay: duration)
    }

    @objc
    func hide() {
        UIView.animate(springDuration: 0.3, bounce: 0.4) {
            self.transform = CGAffineTransform(translationX: 0, y: 2 * frame.height)
        } completion: { _ in
            self.removeFromSuperview()
        }
    }
}

private extension ToastView.ToastType {
    private var iconName: String {
        switch self {
        case .success: "checkmark.circle"
        case .error: "exclamationmark.circle"
        }
    }

    private var symbolConfiguration: UIImage.SymbolConfiguration {
        let size = UIImage.SymbolConfiguration(pointSize: 24)
        switch self {
        case .success:
            let colors = UIImage.SymbolConfiguration(paletteColors: [
                .fgSuccess
            ])
            return colors.applying(size)
        case .error:
            let colors = UIImage.SymbolConfiguration(paletteColors: [
                .fgStaticWhite
            ])
            return colors.applying(size)
        }
    }

    var image: UIImage? {
        switch self {
        case .success:
            UIImage(systemName: iconName, withConfiguration: symbolConfiguration)
        case .error:
            UIImage(systemName: iconName, withConfiguration: symbolConfiguration)
        }
    }

    var backgroundColor: UIColor {
        switch self {
        case .success:
            .bgSurfaceContainer
        case .error:
            .bgStatusError
        }
    }

    var textColor: UIColor {
        switch self {
        case .success:
            .fgPrimary
        case .error:
            .fgStaticWhite
        }
    }
}
