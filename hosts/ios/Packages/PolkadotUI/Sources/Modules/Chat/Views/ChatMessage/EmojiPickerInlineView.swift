import UIKit
import DesignSystem
import ExternalAccessibility

internal import SnapKit

public final class EmojiPickerInlineView: UIView {
    public var onEmojiSelected: ((String) -> Void)?

    private let sections: [EmojiPickerInline.Section]

    private lazy var collectionView: UICollectionView = {
        let layout = UICollectionViewFlowLayout()
        layout.minimumInteritemSpacing = 0
        layout.minimumLineSpacing = 0
        layout.sectionInset = Constants.sectionInset
        layout.headerReferenceSize = CGSize(width: 0, height: Constants.headerHeight)
        let collectionView = UICollectionView(frame: .zero, collectionViewLayout: layout)
        collectionView.backgroundColor = .clear
        return collectionView
    }()

    private enum Constants {
        static let emojiSize: CGFloat = 40
        static let columns: Int = 8
        static let sectionInset = UIEdgeInsets(top: 8, left: 16, bottom: 8, right: 16)
        static let headerHeight: CGFloat = 36
        static let cornerRadius: CGFloat = 24
    }

    public init(sections: [EmojiPickerInline.Section]) {
        self.sections = sections
        super.init(frame: .zero)
        setupView()
        applyAccessibilityBindings()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }
}

// MARK: - Private functions

extension EmojiPickerInlineView {
    private func setupView() {
        backgroundColor = .bgSurfaceContainer
        layer.cornerRadius = Constants.cornerRadius
        layer.masksToBounds = true

        collectionView.dataSource = self
        collectionView.delegate = self
        collectionView.register(InlineEmojiCell.self, forCellWithReuseIdentifier: InlineEmojiCell.reuseId)
        collectionView.register(
            InlineEmojiSectionHeaderView.self,
            forSupplementaryViewOfKind: UICollectionView.elementKindSectionHeader,
            withReuseIdentifier: InlineEmojiSectionHeaderView.reuseId
        )

        addSubview(collectionView)
        collectionView.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }
}

// MARK: - UICollectionViewDataSource

extension EmojiPickerInlineView: UICollectionViewDataSource {
    public func numberOfSections(in _: UICollectionView) -> Int {
        sections.count
    }

    public func collectionView(_: UICollectionView, numberOfItemsInSection section: Int) -> Int {
        sections[section].emojis.count
    }

    public func collectionView(
        _ collectionView: UICollectionView,
        cellForItemAt indexPath: IndexPath
    ) -> UICollectionViewCell {
        guard let cell = collectionView.dequeueReusableCell(
            withReuseIdentifier: InlineEmojiCell.reuseId,
            for: indexPath
        ) as? InlineEmojiCell else {
            return UICollectionViewCell()
        }
        cell.configure(emoji: sections[indexPath.section].emojis[indexPath.item])
        return cell
    }

    public func collectionView(
        _ collectionView: UICollectionView,
        viewForSupplementaryElementOfKind kind: String,
        at indexPath: IndexPath
    ) -> UICollectionReusableView {
        guard let header = collectionView.dequeueReusableSupplementaryView(
            ofKind: kind,
            withReuseIdentifier: InlineEmojiSectionHeaderView.reuseId,
            for: indexPath
        ) as? InlineEmojiSectionHeaderView else {
            return UICollectionReusableView()
        }
        header.configure(title: sections[indexPath.section].title)
        return header
    }
}

// MARK: - UICollectionViewDelegate

extension EmojiPickerInlineView: UICollectionViewDelegate {
    public func collectionView(_: UICollectionView, didSelectItemAt indexPath: IndexPath) {
        let emoji = sections[indexPath.section].emojis[indexPath.item]
        onEmojiSelected?(emoji)
    }
}

// MARK: - UICollectionViewDelegateFlowLayout

extension EmojiPickerInlineView: UICollectionViewDelegateFlowLayout {
    public func collectionView(
        _ collectionView: UICollectionView,
        layout _: UICollectionViewLayout,
        sizeForItemAt _: IndexPath
    ) -> CGSize {
        let insets = Constants.sectionInset
        let availableWidth = collectionView.bounds.width - insets.left - insets.right
        let itemWidth = floor(availableWidth / CGFloat(Constants.columns))
        return CGSize(width: itemWidth, height: Constants.emojiSize)
    }
}

// MARK: - Cell

private final class InlineEmojiCell: UICollectionViewCell {
    static let reuseId = "InlineEmojiCell"
    private let label = UILabel()

    override init(frame: CGRect) {
        super.init(frame: frame)
        label.font = .systemFont(ofSize: 26)
        label.textAlignment = .center
        contentView.addSubview(label)
        label.snp.makeConstraints { make in
            make.edges.equalToSuperview()
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    func configure(emoji: String) {
        label.text = emoji
    }
}

private final class InlineEmojiSectionHeaderView: UICollectionReusableView {
    static let reuseId = "InlineEmojiSectionHeader"
    private let titleLabel = Label()

    override init(frame: CGRect) {
        super.init(frame: frame)
        titleLabel.typography = .labelMediumEmphasized
        titleLabel.textColor = .fgSecondary
        addSubview(titleLabel)
        titleLabel.snp.makeConstraints { make in
            make.leading.equalToSuperview().inset(16)
            make.trailing.equalToSuperview().inset(16)
            make.bottom.equalToSuperview().inset(4)
        }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError()
    }

    func configure(title: String) {
        titleLabel.text = title.uppercased()
    }
}

public enum EmojiPickerInline {
    public struct Section {
        public let title: String
        public let emojis: [String]

        public init(title: String, emojis: [String]) {
            self.title = title
            self.emojis = emojis
        }
    }
}

// MARK: - AccessibilityBound

extension EmojiPickerInlineView: AccessibilityBound {
    public var accessibilityBindings: [AccessibilityBinding] {
        [.init(self, AccessibilityID.Chat.reactionEmojiPanel)]
    }
}
