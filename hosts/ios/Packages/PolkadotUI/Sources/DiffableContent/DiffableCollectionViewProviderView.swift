import Foundation
import UIKit
internal import SnapKit

public class DiffableCollectionViewProviderView<SectionIdType: Hashable, ItemIdType: Hashable>:
    UIView,
    DiffableCollectionViewProviding {
    public typealias ItemIdentifierType = ItemIdType
    public typealias SectionIdentifierType = SectionIdType

    public lazy var layout: UICollectionViewLayout = createLayout()

    public lazy var collectionView: UICollectionView = .init(frame: .zero, collectionViewLayout: layout)

    public lazy var dataSource: DataSourceType = createDataSource()

    public lazy var sectionProviders: [SectionProviderType] = []

    public var itemProviderMap: [ItemIdType: ItemProviderType] = [:]

    public var isEmpty: Bool {
        itemProviderMap.isEmpty
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        baseSetup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        baseSetup()
    }

    open func baseSetup() {
        setupViews()
        registerCells()
        updateSections()
        applySnapshot()
    }

    open func setupViews() {
        collectionView.backgroundColor = .systemBackground
        addSubview(collectionView)
        collectionView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }
    }

    open func createLayout() -> UICollectionViewCompositionalLayout {
        let layout = UICollectionViewCompositionalLayout { [unowned self] section, environment in
            let provider = sectionProviders[section].sectionLayoutProvider
            return provider(section, environment)
        }
        return layout
    }

    open func registerCells() {
        // implement
    }

    open func updateSections() {
        // implement
    }

    public func updateCells() {
        var snapshot = dataSource.snapshot()
        snapshot.reconfigureItems(snapshot.itemIdentifiers)
        dataSource.apply(snapshot)
    }
}
