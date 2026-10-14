import UIKit
internal import SnapKit

public final class ShareContactsGridView: DiffableCollectionViewProviderView<Int, String> {
    override public func setupViews() {
        backgroundColor = .clear
        addSubview(collectionView)
        collectionView.snp.makeConstraints {
            $0.directionalEdges.equalToSuperview()
        }
        collectionView.backgroundColor = .clear
        collectionView.bounces = false
        collectionView.delegate = self
    }

    override public func registerCells() {
        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: SelectableContactView.reuseIdentifier
        )
    }

    public func bind(contacts: [IdentifiableContentConfiguration<String, SelectableContactConfiguration>]) {
        let itemProviders = contacts.map { contact in
            ItemProviderType(
                id: contact.id,
                configuration: contact.configuration,
                reuseIdentifier: SelectableContactView.reuseIdentifier
            )
        }

        let section = SectionProviderType(
            id: 0,
            itemProviders: itemProviders
        ) { _, _ in
            let itemsPerRow = 4

            let item = NSCollectionLayoutItem(
                layoutSize: NSCollectionLayoutSize(
                    widthDimension: .fractionalWidth(1 / CGFloat(itemsPerRow)),
                    heightDimension: .absolute(71)
                )
            )
            let group = NSCollectionLayoutGroup.horizontal(
                layoutSize: NSCollectionLayoutSize(
                    widthDimension: .fractionalWidth(1),
                    heightDimension: .absolute(71)
                ),
                repeatingSubitem: item,
                count: itemsPerRow
            )
            group.interItemSpacing = .fixed(18)
            let section = NSCollectionLayoutSection(group: group)
            section.interGroupSpacing = 18
            return section
        }

        applySnapshot(sections: [section])
    }
}

extension ShareContactsGridView: UICollectionViewDelegate {
    public func collectionView(_: UICollectionView, didSelectItemAt indexPath: IndexPath) {
        guard
            let identifier = dataSource.itemIdentifier(for: indexPath),
            let provider = itemProviderMap[identifier],
            let configuration = provider.configuration as? SelectableContactConfiguration
        else { return }
        configuration.onSelection?(!configuration.isSelected)
    }
}
