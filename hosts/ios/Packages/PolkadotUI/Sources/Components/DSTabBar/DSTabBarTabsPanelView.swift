import UIKit
import DesignSystem

public final class DSTabBarTabsPanelView: UIView {
    public var onChipTapped: ((UUID) -> Void)?
    public var onChipCloseRequested: ((UUID) -> Void)?

    public var closeActionTitle: String = ""

    public private(set) var isOpen = false

    private let scrollView = UIScrollView()
    private var chipViews: [DSTabBarChipView] = []
    private var chips: [DSTabBarChip] = []
    private var selectedId: UUID?

    override public init(frame: CGRect) {
        super.init(frame: frame)

        scrollView.showsVerticalScrollIndicator = false
        scrollView.alpha = 0
        addSubview(scrollView)
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setChips(_ chips: [DSTabBarChip], selected: UUID?) {
        let identityChanged = chips.map(\.id) != self.chips.map(\.id)
        self.chips = chips
        selectedId = selected

        guard !identityChanged else {
            rebuildChipViews()
            return
        }

        zip(chipViews, chips).forEach { view, chip in
            view.apply(chip)
            view.isSelectedTab = chip.id == selected
        }
    }

    public func preferredHeight(availableHeight: CGFloat) -> CGFloat {
        DSTabBarPanelLayout.panelHeight(rowHeights: rowHeights, availableHeight: availableHeight)
    }

    public static let openDuration: TimeInterval = 0.18
    public static let openDampingRatio: CGFloat = 0.9

    /// Adds the panel's open/close animations to `animator` so they stay in lockstep with the
    /// container resize the caller drives; applies them immediately when `animator` is nil.
    public func setOpen(_ open: Bool, animator: UIViewPropertyAnimator?) {
        guard open != isOpen else {
            return
        }
        isOpen = open

        let apply = { [self] in
            scrollView.alpha = open ? 1 : 0
        }

        guard let animator else {
            apply()
            return
        }

        animator.addAnimations(apply)
    }

    override public func layoutSubviews() {
        super.layoutSubviews()

        scrollView.frame = bounds
        layoutChipViews()
    }
}

private extension DSTabBarTabsPanelView {
    func rebuildChipViews() {
        chipViews.forEach { $0.removeFromSuperview() }
        chipViews = chips.map { chip in
            let view = DSTabBarChipView()
            view.apply(chip)
            view.isSelectedTab = chip.id == selectedId
            view.addGestureRecognizer(
                UITapGestureRecognizer(target: self, action: #selector(handleChipTap(_:)))
            )
            view.addInteraction(UIContextMenuInteraction(delegate: self))
            scrollView.addSubview(view)
            return view
        }
        setNeedsLayout()
    }

    var columnWidth: CGFloat {
        let contentWidth = max(0, bounds.width - DSTabBarPanelLayout.horizontalPadding * 2)
        return contentWidth / CGFloat(DSTabBarPanelLayout.columnsPerRow)
    }

    var rowHeights: [CGFloat] {
        let columns = DSTabBarPanelLayout.columnsPerRow
        let width = columnWidth

        return stride(from: 0, to: chips.count, by: columns).map { start in
            let labelHeight = chips[start ..< min(start + columns, chips.count)]
                .map { DSTabBarChipView.fittingLabelHeight(for: $0.name, width: width) }
                .max() ?? DSTabBarPanelLayout.labelLineHeight
            return DSTabBarPanelLayout.rowHeight(labelHeight: labelHeight)
        }
    }

    func rowOffsets(for heights: [CGFloat]) -> [CGFloat] {
        var offsets: [CGFloat] = []
        var offset = DSTabBarPanelLayout.verticalPadding

        for height in heights {
            offsets.append(offset)
            offset += height + DSTabBarPanelLayout.rowSpacing
        }

        return offsets
    }

    func layoutChipViews() {
        let width = columnWidth
        let heights = rowHeights
        let offsets = rowOffsets(for: heights)

        for (index, view) in chipViews.enumerated() {
            let row = index / DSTabBarPanelLayout.columnsPerRow
            let column = index % DSTabBarPanelLayout.columnsPerRow
            view.frame = CGRect(
                x: DSTabBarPanelLayout.horizontalPadding + CGFloat(column) * width,
                y: offsets[row],
                width: width,
                height: heights[row]
            )
        }

        scrollView.contentSize = CGSize(
            width: bounds.width,
            height: DSTabBarPanelLayout.contentHeight(rowHeights: heights)
        )
    }

    @objc func handleChipTap(_ recognizer: UITapGestureRecognizer) {
        guard let id = (recognizer.view as? DSTabBarChipView)?.chip?.id else {
            return
        }
        onChipTapped?(id)
    }
}

// MARK: - UIContextMenuInteractionDelegate

extension DSTabBarTabsPanelView: UIContextMenuInteractionDelegate {
    public func contextMenuInteraction(
        _ interaction: UIContextMenuInteraction,
        configurationForMenuAtLocation _: CGPoint
    ) -> UIContextMenuConfiguration? {
        guard let id = (interaction.view as? DSTabBarChipView)?.chip?.id else {
            return nil
        }

        return UIContextMenuConfiguration(identifier: nil, previewProvider: nil) { [weak self] _ in
            let close = UIAction(
                title: self?.closeActionTitle ?? "",
                image: UIImage(systemName: "xmark"),
                attributes: .destructive
            ) { _ in
                self?.onChipCloseRequested?(id)
            }
            return UIMenu(children: [close])
        }
    }
}
