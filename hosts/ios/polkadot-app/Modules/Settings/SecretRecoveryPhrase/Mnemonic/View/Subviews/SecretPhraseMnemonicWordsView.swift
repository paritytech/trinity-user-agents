import UIKit
import UIKit_iOS
import SnapKit
import PolkadotUI
import DesignSystem

protocol SecretPhraseMnemonicWordsDelegate: AnyObject {
    func didTapMnemonic(_ type: SecretPhraseMnemonicWordsView.ViewState)
}

final class SecretPhraseMnemonicWordsView: UIView {
    // MARK: Properties

    enum ViewState {
        case shown
        case hidden
    }

    private typealias Snapshot = NSDiffableDataSourceSnapshot<
        SecretPhraseMnemonicViewModel.Section,
        SecretPhraseMnemonicViewModel.Cell
    >
    private typealias DataSource = UICollectionViewDiffableDataSource<
        SecretPhraseMnemonicViewModel.Section,
        SecretPhraseMnemonicViewModel.Cell
    >
    weak var delegate: SecretPhraseMnemonicWordsDelegate?

    private let blurView: UIView = .create { view in
        view.backgroundColor = .clear
    }

    let hintView: GenericPairValueView<Label, Label> = .create { view in
        view.setVerticalAndSpacing(8)
        view.fView.typography = .titleMedium
        view.fView.textColor = .fgPrimary
        view.fView.textAlignment = .center
        view.sView.typography = .bodyMedium
        view.sView.textColor = .fgSecondary
        view.sView.numberOfLines = 3
        view.sView.textAlignment = .center
    }

    var titleLabel: UILabel {
        hintView.fView
    }

    var subtitleLabel: UILabel {
        hintView.sView
    }

    private lazy var collectionView = UICollectionView(frame: .zero, collectionViewLayout: createLayout())
    private lazy var dataSource = configureDataSource()

    var currentType: ViewState = .hidden {
        didSet {
            update(for: currentType)
        }
    }

    override var intrinsicContentSize: CGSize {
        let totalHeight = (CGFloat(Constants.numberOfRows) * Constants.itemHeight) +
            (CGFloat(Constants.numberOfRows - 1) * Constants.spacing) + Constants.insets * 2
        return CGSize(width: UIView.noIntrinsicMetric, height: totalHeight)
    }

    // MARK: Initial methods

    override init(frame: CGRect) {
        super.init(frame: frame)
        configureView()
        update(for: currentType)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    // MARK: Public methods

    func bind(viewModel: SecretPhraseMnemonicViewModel) {
        let itemCount = viewModel.cells.count
        Constants.numberOfRows = Int(ceil(Double(itemCount) / 3.0))

        applySnapshot(viewModel.cells)
        invalidateIntrinsicContentSize()
    }

    func fetchMnemonic() -> String {
        let snapshot = dataSource.snapshot()
        let cells = snapshot.itemIdentifiers
        let mnemonicWords = cells.map(\.text)

        return mnemonicWords.joined(with: .space)
    }
}

// MARK: Private methods

extension SecretPhraseMnemonicWordsView {
    private func update(for state: ViewState) {
        blurView.isHidden = (state != .hidden)
        collectionView.isHidden = (state == .hidden)
        delegate?.didTapMnemonic(state)
    }

    private func applySnapshot(_ cells: [SecretPhraseMnemonicViewModel.Cell]) {
        var snapshot = Snapshot()
        snapshot.appendSections([.main])
        snapshot.appendItems(cells)
        dataSource.apply(snapshot)
    }

    private func createLayout() -> UICollectionViewLayout {
        let itemSize = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1 / 3),
            heightDimension: .absolute(Constants.itemHeight)
        )
        let item = NSCollectionLayoutItem(layoutSize: itemSize)

        let groupSize = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1),
            heightDimension: .absolute(Constants.itemHeight)
        )
        let group = NSCollectionLayoutGroup.horizontal(
            layoutSize: groupSize,
            repeatingSubitem: item,
            count: 3
        )
        group.interItemSpacing = .fixed(Constants.spacing)

        let section = NSCollectionLayoutSection(group: group)
        section.interGroupSpacing = Constants.spacing
        section.contentInsets = NSDirectionalEdgeInsets(
            top: Constants.insets,
            leading: Constants.insets,
            bottom: Constants.insets,
            trailing: Constants.insets
        )
        return UICollectionViewCompositionalLayout(section: section)
    }

    private func configureDataSource() -> DataSource {
        DataSource(collectionView: collectionView) { collectionView, indexPath, item in
            let cell = collectionView.dequeueReusableCellWithType(MnemonicWordCollectionCell.self, for: indexPath)
            cell?.bind(model: item)

            return cell
        }
    }

    private func configureView() {
        collectionView.backgroundColor = .bgSurfaceContainer
        if #available(iOS 26.0, *) {
            collectionView.cornerConfiguration = .uniformCorners(
                radius: .fixed(Constants.itemHeight / 2)
            )
        } else {
            collectionView.layer.cornerRadius = Constants.itemHeight / 2
        }

        collectionView.alwaysBounceVertical = true
        collectionView.isScrollEnabled = false
        collectionView.registerCellClass(MnemonicWordCollectionCell.self)

        addSubview(collectionView)
        addSubview(blurView)

        collectionView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        blurView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        blurView.addSubview(hintView)
        hintView.snp.makeConstraints { make in
            make.center.equalToSuperview()
            make.leading.trailing.lessThanOrEqualToSuperview().inset(24)
        }
    }
}

private extension SecretPhraseMnemonicWordsView {
    enum Constants {
        static var numberOfRows = 4
        static let itemHeight: CGFloat = 32
        static let spacing: CGFloat = 12
        static let insets: CGFloat = 12
    }
}
