import UIKit
public import UIKit_iOS

open class CollectionViewContainerCell<ContentView: UIView>: UICollectionViewCell {
    public let view = ContentView()

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupStyle()
        setupLayout()
    }

    @available(*, unavailable)
    public required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setupStyle() {}

    func setupLayout() {
        contentView.addSubview(view)
        view.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }
}

open class CollectionViewContainerBackgroundCell<ContentView: UIView>:
    CollectionViewContainerCell<GenericBorderedView<ContentView>> {
    public var containerBackgroundView: RoundedView {
        view.backgroundView
    }

    public var displayContentView: ContentView {
        view.contentView
    }
}
