import UIKit
public import UIKit_iOS

open class IconDetailsGenericView<Details: UIView>: UIView {
    public enum Mode {
        case iconDetails
        case detailsIcon
    }

    public let imageView: UIImageView = {
        let imageView = UIImageView()
        imageView.contentMode = .center
        return imageView
    }()

    public let detailsView: Details

    public var mode: Mode = .iconDetails {
        didSet {
            applyLayout()
        }
    }

    public var spacing: CGFloat {
        get {
            stackView.spacing
        }

        set {
            stackView.spacing = newValue
        }
    }

    private(set) var stackView: UIStackView = {
        let view = UIStackView()
        view.axis = .horizontal
        view.spacing = 8.0
        view.alignment = .center
        return view
    }()

    public var iconWidth: CGFloat = Constants.iconWidth {
        didSet {
            imageView.snp.updateConstraints { make in
                make.width.equalTo(iconWidth)
            }
            setNeedsLayout()
        }
    }

    public init(detailsView: Details = Details()) {
        self.detailsView = detailsView

        super.init(frame: .zero)

        setupLayout()
    }

    override public init(frame: CGRect) {
        detailsView = Details()

        super.init(frame: frame)

        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override public func prepareForInterfaceBuilder() {
        super.prepareForInterfaceBuilder()

        guard imageView.superview == nil else {
            return
        }

        setupLayout()
    }

    private func setupLayout() {
        addSubview(stackView)
        stackView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }

        imageView.snp.makeConstraints { make in
            make.width.equalTo(iconWidth)
        }

        applyLayout()
    }

    private func applyLayout() {
        imageView.removeFromSuperview()
        detailsView.removeFromSuperview()

        switch mode {
        case .iconDetails:
            stackView.addArrangedSubview(imageView)
            stackView.addArrangedSubview(detailsView)
        case .detailsIcon:
            stackView.addArrangedSubview(detailsView)
            stackView.addArrangedSubview(imageView)
        }
    }
}

extension IconDetailsGenericView: Highlightable {
    public func set(highlighted: Bool, animated: Bool) {
        imageView.set(highlighted: highlighted, animated: animated)

        if let highlightableDetails = detailsView as? Highlightable {
            highlightableDetails.set(highlighted: highlighted, animated: animated)
        }
    }
}

class LoadableGenericIconDetailsView<V: UIView>: IconDetailsGenericView<V> {
    private var imageViewModel: ImageViewModelProtocol?

    func bind(imageViewModel: ImageViewModelProtocol?) {
        self.imageViewModel?.cancel(on: imageView)
        imageView.image = nil

        self.imageViewModel = imageViewModel

        imageViewModel?.loadImage(
            on: imageView,
            targetSize: CGSize(width: iconWidth, height: iconWidth),
            animated: true
        )
    }
}

// MARK: - Constants

private enum Constants {
    static let iconWidth: CGFloat = 16
}
