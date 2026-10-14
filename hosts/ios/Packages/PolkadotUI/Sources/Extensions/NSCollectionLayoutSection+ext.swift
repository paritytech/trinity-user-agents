import UIKit

extension NSCollectionLayoutSection {
    static func autoHeightSingleItem(_ estimatedHeight: CGFloat) -> NSCollectionLayoutSection {
        let group = NSCollectionLayoutGroup.autoHeightSingleItem(estimatedHeight)
        return NSCollectionLayoutSection(group: group)
    }

    static func list(
        _ heightDimension: NSCollectionLayoutDimension,
        wightDimension: NSCollectionLayoutDimension? = nil
    ) -> NSCollectionLayoutSection {
        let group = NSCollectionLayoutGroup.list(
            heightDimension: heightDimension,
            widthDimension: wightDimension
        )
        return NSCollectionLayoutSection(group: group)
    }

    static func fallback() -> NSCollectionLayoutSection {
        assertionFailure("unexpected behaviour")
        return .list(.fractionalWidth(1))
    }
}

extension NSCollectionLayoutGroup {
    static func autoHeightSingleItem(_ estimatedHeight: CGFloat) -> NSCollectionLayoutGroup {
        let itemSize = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1),
            heightDimension: .estimated(estimatedHeight)
        )
        let item = NSCollectionLayoutItem(layoutSize: itemSize)
        let groupSize = itemSize
        return NSCollectionLayoutGroup.vertical(layoutSize: groupSize, subitems: [item])
    }

    static func list(
        heightDimension: NSCollectionLayoutDimension,
        widthDimension: NSCollectionLayoutDimension? = nil
    ) -> NSCollectionLayoutGroup {
        let itemSize = NSCollectionLayoutSize(
            widthDimension: widthDimension ?? .fractionalWidth(1),
            heightDimension: heightDimension
        )
        let item = NSCollectionLayoutItem(layoutSize: itemSize)
        let groupSize = itemSize
        return NSCollectionLayoutGroup.horizontal(layoutSize: groupSize, subitems: [item])
    }
}

extension NSCollectionLayoutSize {
    static func fill() -> NSCollectionLayoutSize {
        NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1),
            heightDimension: .fractionalHeight(1)
        )
    }
}
