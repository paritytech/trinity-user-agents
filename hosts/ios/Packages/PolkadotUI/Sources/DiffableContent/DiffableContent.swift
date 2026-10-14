import UIKit

// MARK: - Hashable UIContentConfiguration

public protocol HashableContentConfiguration: UIContentConfiguration, Hashable {}

public extension HashableContentConfiguration {
    static var defaultReuseIdentifier: String {
        String(reflecting: Self.self)
    }

    var defaultReuseIdentifier: String {
        String(reflecting: type(of: self))
    }
}

public extension HashableContentConfiguration {
    func updated(for _: UIConfigurationState) -> Self {
        self
    }
}

// MARK: - Provider Types

public struct DiffableContentProvider<ItemId: Hashable>: Hashable, Identifiable {
    public var id: ItemId
    var configuration: any HashableContentConfiguration
    let reuseIdentifier: String
    let extraConfigure: ((UICollectionViewCell) -> Void)?

    init(
        id: ItemId,
        configuration: some HashableContentConfiguration,
        reuseIdentifier: String,
        extraConfigure: ((UICollectionViewCell) -> Void)? = nil
    ) {
        self.id = id
        self.configuration = configuration
        self.reuseIdentifier = reuseIdentifier
        self.extraConfigure = extraConfigure
    }

    func provideCell(in collectionView: UICollectionView, at indexPath: IndexPath) -> UICollectionViewCell {
        let cell = collectionView.dequeueReusableCell(withReuseIdentifier: reuseIdentifier, for: indexPath)
        cell.contentConfiguration = configuration
        extraConfigure?(cell)
        return cell
    }

    func provideSupplementary(
        in collectionView: UICollectionView,
        kind: String,
        at indexPath: IndexPath
    ) -> UICollectionReusableView {
        let view = collectionView.dequeueReusableSupplementaryView(
            ofKind: kind,
            withReuseIdentifier: reuseIdentifier,
            for: indexPath
        )

        guard let cell = view as? UICollectionViewCell else {
            assertionFailure("Supplementary view should be a UICollectionViewCell")
            return view
        }
        cell.contentConfiguration = configuration

        extraConfigure?(cell)
        return cell
    }

    // MARK: - Hashable

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.id == rhs.id
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

public struct DiffableContentSectionProvider<SectionId: Hashable, ItemId: Hashable>: Hashable, Identifiable {
    public var id: SectionId
    var itemProviders: [DiffableContentProvider<ItemId>]
    var sectionLayoutProvider: UICollectionViewCompositionalLayoutSectionProvider
    var supplementaryProviders: [String: DiffableContentProvider<ItemId>]

    init(
        id: SectionId,
        itemProviders: [DiffableContentProvider<ItemId>],
        sectionLayoutProvider: @escaping UICollectionViewCompositionalLayoutSectionProvider,
        supplementaryProviders: [String: DiffableContentProvider<ItemId>] = [:]
    ) {
        self.id = id
        self.itemProviders = itemProviders
        self.sectionLayoutProvider = sectionLayoutProvider
        self.supplementaryProviders = supplementaryProviders
    }

    func provideSupplementary(
        in collectionView: UICollectionView,
        kind: String,
        at indexPath: IndexPath
    ) -> UICollectionReusableView? {
        guard let provider = supplementaryProviders[kind] else { return nil }
        return provider.provideSupplementary(in: collectionView, kind: kind, at: indexPath)
    }

    // MARK: - Hashable

    public static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.id == rhs.id
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

// MARK: - Diffable Collection Provider (Identifier-first)

public protocol DiffableCollectionViewProviding: AnyObject {
    associatedtype SectionIdentifierType: Hashable
    associatedtype ItemIdentifierType: Hashable

    typealias DataSourceType = UICollectionViewDiffableDataSource<SectionIdentifierType, ItemIdentifierType>
    typealias SnapshotType = NSDiffableDataSourceSnapshot<SectionIdentifierType, ItemIdentifierType>
    typealias SectionProviderType = DiffableContentSectionProvider<SectionIdentifierType, ItemIdentifierType>
    typealias ItemProviderType = DiffableContentProvider<ItemIdentifierType>

    var layout: UICollectionViewLayout { get set }
    var collectionView: UICollectionView { get set }
    var dataSource: DataSourceType { get set }
    var sectionProviders: [SectionProviderType] { get set }

    var itemProviderMap: [ItemIdentifierType: ItemProviderType] { get set }

    func createLayout() -> UICollectionViewCompositionalLayout
    func createDataSource() -> DataSourceType
    func applySnapshot(completion: (() -> Void)?)
    func applySnapshot(sections: [SectionProviderType], completion: (() -> Void)?)
    func rebuildProviderMap()
}

public extension DiffableCollectionViewProviding {
    func rebuildProviderMap() {
        itemProviderMap.removeAll(keepingCapacity: true)
        for section in sectionProviders {
            for provider in section.itemProviders {
                itemProviderMap[provider.id] = provider
            }
        }
    }

    func createDataSource() -> DataSourceType {
        let cellProvider: DataSourceType.CellProvider = { [weak self] collectionView, indexPath, identifier in
            guard let self,
                  let provider = itemProviderMap[identifier] else {
                return nil
            }
            return provider.provideCell(in: collectionView, at: indexPath)
        }

        let supplementaryProvider: DataSourceType
            .SupplementaryViewProvider = { [weak self] collectionView, kind, indexPath in
                guard let self else { return nil }
                let section = sectionProviders[indexPath.section]
                return section.provideSupplementary(in: collectionView, kind: kind, at: indexPath)
            }

        let dataSource = DataSourceType(collectionView: collectionView, cellProvider: cellProvider)
        dataSource.supplementaryViewProvider = supplementaryProvider
        return dataSource
    }

    // Simple apply for current state
    func applySnapshot(completion: (() -> Void)? = nil) {
        rebuildProviderMap()

        var snapshot = SnapshotType()
        for section in sectionProviders {
            snapshot.appendSections([section.id])
            snapshot.appendItems(section.itemProviders.map(\.id))
        }
        dataSource.apply(snapshot, completion: completion)
    }

    // Single-pass apply with reconfigure based on configuration hash
    func applySnapshot(
        sections: [SectionProviderType],
        completion: (() -> Void)? = nil
    ) {
        let beforeProviders = sectionProviders.flatMap(\.itemProviders)
        var beforeMap: [ItemIdentifierType: ItemProviderType] = [:]
        beforeProviders.forEach { beforeMap[$0.id] = $0 }

        sectionProviders = sections

        rebuildProviderMap()

        var snapshot = SnapshotType()
        for section in sectionProviders {
            snapshot.appendSections([section.id])
            snapshot.appendItems(section.itemProviders.map(\.id))
        }

        // Detect items whose configuration changed
        var reconfigureIds: [ItemIdentifierType] = []
        var reloadIds: [ItemIdentifierType] = []
        for section in sectionProviders {
            for after in section.itemProviders {
                guard let before = beforeMap[after.id] else {
                    continue
                }
                guard AnyHashable(before.configuration) != AnyHashable(after.configuration) else {
                    continue
                }
                if before.reuseIdentifier != after.reuseIdentifier {
                    reloadIds.append(after.id)
                } else {
                    reconfigureIds.append(after.id)
                }
            }
        }

        snapshot.reconfigureItems(reconfigureIds)
        snapshot.reloadItems(reloadIds)

        dataSource.apply(snapshot, animatingDifferences: true, completion: completion)
    }

    func appendSection(_ section: SectionProviderType) {
        sectionProviders.append(section)

        for provider in section.itemProviders {
            itemProviderMap[provider.id] = provider
        }

        var snapshot = dataSource.snapshot()

        snapshot.appendSections([section.id])
        snapshot.appendItems(section.itemProviders.map(\.id))

        dataSource.apply(snapshot, animatingDifferences: true)
    }

    func removeSection(id: SectionIdentifierType) {
        guard let index = sectionProviders.firstIndex(where: { $0.id == id }) else {
            return
        }
        let removedSection = sectionProviders.remove(at: index)
        for provider in removedSection.itemProviders {
            itemProviderMap.removeValue(forKey: provider.id)
        }

        var snapshot = dataSource.snapshot()

        snapshot.deleteSections([id])

        dataSource.apply(snapshot, animatingDifferences: true)
    }

    func updateSectionData(section: SectionProviderType) {
        guard let index = sectionProviders.firstIndex(where: { $0.id == section.id }) else {
            return
        }
        let oldSection = sectionProviders[index]
        let oldKeys = Set(oldSection.itemProviders.map(\.id))
        let newKeys = Set(section.itemProviders.map(\.id))
        let commonKeys = oldKeys.intersection(newKeys)

        for provider in oldSection.itemProviders {
            itemProviderMap.removeValue(forKey: provider.id)
        }

        sectionProviders[index] = section
        for provider in section.itemProviders {
            itemProviderMap[provider.id] = provider
        }

        var snapshot = dataSource.snapshot()
        snapshot.deleteItems(snapshot.itemIdentifiers(inSection: section.id))
        snapshot.appendItems(section.itemProviders.map(\.id), toSection: section.id)
        snapshot.reconfigureItems(Array(commonKeys))
        dataSource.apply(snapshot, animatingDifferences: true)
    }
}

// MARK: - Registration Helpers

enum CollectionRegistration {
    static func registerCell(
        _ cell: (some UICollectionViewCell).Type,
        for collectionView: UICollectionView,
        reuseId: String
    ) {
        collectionView.register(cell, forCellWithReuseIdentifier: reuseId)
    }

    static func registerSupplementary(
        _ view: (some UICollectionReusableView).Type,
        kind: String,
        for collectionView: UICollectionView,
        reuseId: String
    ) {
        collectionView.register(view, forSupplementaryViewOfKind: kind, withReuseIdentifier: reuseId)
    }
}
