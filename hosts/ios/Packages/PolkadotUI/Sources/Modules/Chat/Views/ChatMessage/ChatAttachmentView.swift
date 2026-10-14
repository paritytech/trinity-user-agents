import UIKit
internal import SnapKit

public final class MediaAttachmentView: DiffableCollectionViewProviderView<Int, String> {
    private static let mainSection = 0
    private static let cellReuseIdentifier = ChatMessageMediaViewConfiguration.defaultReuseIdentifier

    override public func setupViews() {
        super.setupViews()
        collectionView.backgroundColor = .clear
        collectionView.isScrollEnabled = false

        setContentHuggingPriority(.required, for: .horizontal)
        setContentHuggingPriority(.required, for: .vertical)
        setContentCompressionResistancePriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .vertical)
    }

    override public func registerCells() {
        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: Self.cellReuseIdentifier
        )
    }

    public func configure(with items: [ChatRichTextMessageConfiguration.AttachmentItem]) {
        let itemProviders: [ItemProviderType] = items.map { item in
            ItemProviderType(
                id: item.identifier,
                configuration: item.mediaConfiguration,
                reuseIdentifier: Self.cellReuseIdentifier
            )
        }

        let gridSection = Self.makeGridSection(itemCount: items.count)
        let section = SectionProviderType(
            id: Self.mainSection,
            itemProviders: itemProviders
        ) { _, _ in gridSection }

        applySnapshot(sections: [section])
        invalidateIntrinsicContentSize()
    }

    override public var intrinsicContentSize: CGSize {
        let count = sectionProviders.first?.itemProviders.count ?? 0
        switch count {
        case 0:
            return .zero
        case 1:
            return CGSize(width: 200, height: 200)
        case 2:
            return CGSize(width: 240, height: 160)
        case 3:
            return CGSize(width: 200, height: 160)
        case 4:
            return CGSize(width: 200, height: 200)
        default:
            let rowsCount = CGFloat((count + 1) / 2)
            return CGSize(width: 200, height: rowsCount * 100)
        }
    }
}

private extension MediaAttachmentView {
    enum Constants {
        static let spacing: CGFloat = 2
        static let bodyCornerRadius: CGFloat = 12
        static let tailCornerRadius: CGFloat = 2
    }

    // MARK: - Grid layout

    static func makeGridSection(itemCount: Int) -> NSCollectionLayoutSection {
        let spacing = Constants.spacing
        let group: NSCollectionLayoutGroup =
            switch itemCount {
            case 0,
                 1:
                makeSingleItemGroup()
            case 2:
                makePairRow(height: .fractionalHeight(1), spacing: spacing)
            case 3:
                makeThreeItemGroup(spacing: spacing)
            case 4:
                makeVerticalStack(pairCount: 2, hasOddTail: false, spacing: spacing)
            default:
                makeVerticalStack(
                    pairCount: itemCount / 2,
                    hasOddTail: itemCount % 2 != 0,
                    spacing: spacing
                )
            }

        return NSCollectionLayoutSection(group: group)
    }

    // MARK: - Group helpers

    /// Single item filling the full available size.
    static func makeSingleItemGroup() -> NSCollectionLayoutGroup {
        let item = NSCollectionLayoutItem(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .fractionalHeight(1))
        )
        return NSCollectionLayoutGroup.horizontal(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .fractionalHeight(1)),
            subitems: [item]
        )
    }

    /// Two equal items side by side at the given row height.
    static func makePairRow(height: NSCollectionLayoutDimension, spacing: CGFloat) -> NSCollectionLayoutGroup {
        let item = NSCollectionLayoutItem(
            layoutSize: .init(widthDimension: .fractionalWidth(0.5), heightDimension: .fractionalHeight(1))
        )
        let row = NSCollectionLayoutGroup.horizontal(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: height),
            subitems: [item, item]
        )
        row.interItemSpacing = .fixed(spacing)
        return row
    }

    /// One item spanning the full row width at the given row height.
    static func makeSingleRow(height: NSCollectionLayoutDimension) -> NSCollectionLayoutGroup {
        let item = NSCollectionLayoutItem(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .fractionalHeight(1))
        )
        return NSCollectionLayoutGroup.horizontal(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: height),
            subitems: [item]
        )
    }

    /// Large item on the left, two stacked items on the right, filling the full available size.
    static func makeThreeItemGroup(spacing: CGFloat) -> NSCollectionLayoutGroup {
        let leftItem = NSCollectionLayoutItem(
            layoutSize: .init(widthDimension: .fractionalWidth(0.5), heightDimension: .fractionalHeight(1))
        )
        let rightItem = NSCollectionLayoutItem(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .fractionalHeight(0.5))
        )
        let rightColumn = NSCollectionLayoutGroup.vertical(
            layoutSize: .init(widthDimension: .fractionalWidth(0.5), heightDimension: .fractionalHeight(1)),
            subitems: [rightItem, rightItem]
        )
        rightColumn.interItemSpacing = .fixed(spacing)
        let group = NSCollectionLayoutGroup.horizontal(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .fractionalHeight(1)),
            subitems: [leftItem, rightColumn]
        )
        group.interItemSpacing = .fixed(spacing)
        return group
    }

    /// Vertical stack of pair rows with an optional trailing single-item row,
    /// filling the full available height — each row gets an equal 1/n share.
    static func makeVerticalStack(pairCount: Int, hasOddTail: Bool, spacing: CGFloat) -> NSCollectionLayoutGroup {
        let totalRowCount = pairCount + (hasOddTail ? 1 : 0)
        let rowHeight = NSCollectionLayoutDimension.fractionalHeight(1.0 / CGFloat(totalRowCount))

        var rows = Array(repeating: makePairRow(height: rowHeight, spacing: spacing), count: pairCount)
        if hasOddTail { rows.append(makeSingleRow(height: rowHeight)) }

        let group = NSCollectionLayoutGroup.vertical(
            layoutSize: .init(widthDimension: .fractionalWidth(1), heightDimension: .fractionalHeight(1)),
            subitems: rows
        )
        group.interItemSpacing = .fixed(spacing)
        return group
    }
}
