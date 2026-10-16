import UIKit
import DesignSystem

public final class DSTabBarView: UIView {
    /// What a press at a given x resolves to. Actions are hit-tested against their own frame;
    /// tabs snap to the nearest centre, so the gaps around an action still reach a tab.
    enum Target: Equatable {
        case tab(Int)
        case action(Int)
    }

    private var itemsStorage: [DSTabBarItem] = []

    public var items: [DSTabBarItem] {
        get { itemsStorage }
        set {
            let previousCount = itemsStorage.count
            itemsStorage = newValue
            rebuildItemViews()

            guard previousCount > 0, newValue.count != previousCount else {
                return
            }
            applyRowReflow()
        }
    }

    public var selectedIndex: Int = 0 {
        didSet {
            guard selectedIndex != oldValue else {
                return
            }
            updateLens(animated: true)
            rebuildAccessibilityElements()
        }
    }

    /// The action item whose panel is open. Tints that item without moving the lens.
    public var activeActionIndex: Int? {
        didSet {
            guard activeActionIndex != oldValue else {
                return
            }
            applyActiveAction()
            rebuildAccessibilityElements()
        }
    }

    public var onSelect: ((_ index: Int, _ isReselection: Bool) -> Void)?

    public var onActionTapped: ((Int) -> Void)?

    public var onFoldChangeRequested: ((_ folded: Bool, _ velocityX: CGFloat) -> Void)?

    /// Fires once item frames are assigned, so callers anchoring to them can wait for real
    /// geometry rather than guessing when layout has run.
    public var onItemsLaidOut: (() -> Void)?

    private let content = UIView()
    private let lens = DSTabBarSelectionLens()
    private let keyboardShadow = DSTabBarKeyboardShadowView()

    private var itemViews: [DSTabBarItemView] = []
    private var selectedItemViews: [DSTabBarItemView] = []

    private var dragState: (startX: CGFloat, currentX: CGFloat, index: Int)?

    public var isFolded: Bool = false

    private var animatesLensOnNextLayout = false

    public static func preferredHeight() -> CGFloat {
        DSTabBarMetrics.capsuleHeight + DSTabBarMetrics.bottomGap
    }

    public static var horizontalMargin: CGFloat { DSTabBarMetrics.horizontalMargin }
    public static var maxWidth: CGFloat { DSTabBarMetrics.maxWidth }
    public static var capsuleHeight: CGFloat { DSTabBarMetrics.capsuleHeight }
    public static var bottomGap: CGFloat { DSTabBarMetrics.bottomGap }
    public static var foldGrabZoneWidth: CGFloat { DSTabBarMetrics.foldGrabZoneWidth }

    /// Distance from the screen's leading edge to the chrome capsule's leading edge.
    public static func horizontalInset(availableWidth: CGFloat) -> CGFloat {
        DSTabBarGeometry.capsuleFrame(availableWidth: availableWidth).minX
    }

    public static func foldedTranslationX(availableWidth: CGFloat) -> CGFloat {
        DSTabBarGeometry.foldedTranslationX(availableWidth: availableWidth)
    }

    public static func hiddenTranslationX(availableWidth: CGFloat) -> CGFloat {
        DSTabBarGeometry.hiddenTranslationX(availableWidth: availableWidth)
    }

    override public init(frame: CGRect) {
        super.init(frame: frame)

        setupHierarchy()
        setupGesture()
        setupAccessibility()
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    public func setBadge(_ badge: DSTabBarItem.Badge?, at index: Int) {
        guard itemsStorage.indices.contains(index) else {
            return
        }
        itemsStorage[index].badge = badge
        itemViews[index].apply(itemsStorage[index])
        selectedItemViews[index].apply(itemsStorage[index])
        applyActiveAction()
    }

    /// Shown while the capsule sits under the keyboard, so the keys read as being above it.
    public func setKeyboardShadowVisible(_ visible: Bool) {
        keyboardShadow.alpha = visible ? 1 : 0
    }

    /// Anchor view for a popover pointing at a bar item. Re-read it rather than caching:
    /// item views are recreated whenever `items` changes. Nil until frames are assigned in
    /// `layoutSubviews` — an empty source rect makes UIKit centre the popover on screen.
    public func itemAnchor(at index: Int) -> UIView? {
        guard itemViews.indices.contains(index), !itemViews[index].frame.isEmpty else {
            return nil
        }

        return itemViews[index]
    }

    override public func layoutSubviews() {
        super.layoutSubviews()

        content.frame = bounds
        keyboardShadow.frame = bounds

        let inset = DSTabBarMetrics.innerInset
        lens.frame = content.bounds.insetBy(dx: inset, dy: inset)

        layoutItemViews()
        updateLens(animated: animatesLensOnNextLayout)
        animatesLensOnNextLayout = false
    }

    override public func hitTest(_ point: CGPoint, with _: UIEvent?) -> UIView? {
        guard isUserInteractionEnabled, !isHidden, alpha > 0.01 else {
            return nil
        }

        return content.frame.contains(point) ? self : nil
    }
}

private extension DSTabBarView {
    var row: DSTabBarRow {
        DSTabBarRow(
            width: DSTabBarGeometry.rowWidth(capsuleWidth: bounds.width),
            itemCount: items.count
        )
    }

    var tabIndices: [Int] {
        itemsStorage.indices.filter { itemsStorage[$0].role == .tab }
    }

    /// The apps action sits mid-row, so springing the reflow slides its neighbours across a whole
    /// slot and reads as the whole bar moving. The new layout is applied outright instead.
    func applyRowReflow() {
        setNeedsLayout()
        layoutIfNeeded()
    }

    func setupHierarchy() {
        addSubview(content)
        content.addSubview(lens)
        addSubview(keyboardShadow)
    }

    func setupGesture() {
        let recognizer = DSTabSelectionRecognizer(target: self, action: #selector(handleSelection(_:)))
        addGestureRecognizer(recognizer)
    }

    func setupAccessibility() {
        accessibilityTraits = .tabBar
        isAccessibilityElement = false
    }

    func rebuildItemViews() {
        (itemViews + selectedItemViews).forEach { $0.removeFromSuperview() }

        itemViews = items.map { item in
            let view = DSTabBarItemView(isSelectedAppearance: false)
            view.apply(item)
            lens.contentView.addSubview(view)
            return view
        }
        selectedItemViews = items.map { item in
            let view = DSTabBarItemView(isSelectedAppearance: true)
            view.apply(item)
            lens.selectedContentView.addSubview(view)
            return view
        }

        applyActiveAction()
        rebuildAccessibilityElements()
        setNeedsLayout()
    }

    func layoutItemViews() {
        let row = row
        for index in items.indices {
            let frame = row.itemFrame(at: index)
            itemViews[index].frame = frame
            selectedItemViews[index].frame = frame
        }
        rebuildAccessibilityElements()

        if !items.isEmpty, bounds.width > 0 {
            onItemsLaidOut?()
        }
    }

    func applyActiveAction() {
        for index in itemViews.indices {
            itemViews[index].isActive = index == activeActionIndex
        }
    }

    func updateLens(animated: Bool) {
        guard !items.isEmpty, bounds.width > 0 else {
            return
        }

        lens.update(pillFrame: lensPillFrame(), isLifted: dragState != nil, animated: animated)
    }

    /// The pill rests on the selected tab; a drag in flight carries it, clamped to the row.
    func lensPillFrame() -> CGRect {
        let row = row

        guard let dragState else {
            return row.pillFrame(at: selectedIndex)
        }

        var frame = row.pillFrame(at: dragState.index)
        frame.origin.x = DSTabBarGeometry.clampedPillOriginX(
            dragState.currentX,
            pillWidth: frame.width,
            rowWidth: row.width
        )
        return frame
    }

    func drawnPillIndex(currentX: CGFloat, referenceIndex: Int) -> Int {
        let row = row
        let pillWidth = row.pillFrame(at: referenceIndex).width
        let drawnOriginX = DSTabBarGeometry.clampedPillOriginX(currentX, pillWidth: pillWidth, rowWidth: row.width)
        return row.nearestItemIndex(
            toX: drawnOriginX + pillWidth / 2,
            restrictedTo: tabIndices
        ) ?? referenceIndex
    }

    @objc func handleSelection(_ recognizer: DSTabSelectionRecognizer) {
        switch recognizer.state {
        case .began:
            guard !isFolded, case let .tab(index)? = resolvedTarget(atX: recognizer.location(in: lens).x) else {
                return
            }
            let pill = row.pillFrame(at: index)
            dragState = (startX: pill.minX, currentX: pill.minX, index: index)
            updateLens(animated: true)
        case .changed:
            guard var state = dragState else {
                return
            }
            state.currentX = state.startX + recognizer.translation(in: self).x
            state.index = drawnPillIndex(currentX: state.currentX, referenceIndex: state.index)
            dragState = state
            updateLens(animated: false)
        case .ended,
             .cancelled,
             .failed:
            let dragIndex = dragState?.index
            dragState = nil

            guard recognizer.state == .ended else {
                updateLens(animated: true)
                return
            }
            applyEndedDecision(for: recognizer, dragIndex: dragIndex)
        default:
            break
        }
    }

    func applyEndedDecision(for recognizer: DSTabSelectionRecognizer, dragIndex: Int?) {
        let velocityX = recognizer.velocity(in: self).x
        let decision = DSTabBarFoldDecision.decide(
            velocityX: velocityX,
            translationX: recognizer.translation(in: self).x,
            isFolded: isFolded,
            foldDistance: DSTabBarGeometry.foldedTranslationX(availableWidth: bounds.width)
        )

        switch decision {
        case .select:
            applySelectDecision(for: recognizer, dragIndex: dragIndex)
        case .fold,
             .unfold:
            updateLens(animated: true)
            onFoldChangeRequested?(decision == .fold, velocityX)
        case .settle:
            updateLens(animated: true)
        }
    }

    /// A drag only ever exists for a tab, so a select with no drag is a press on an action.
    func applySelectDecision(for recognizer: DSTabSelectionRecognizer, dragIndex: Int?) {
        if let dragIndex {
            commitSelection(at: dragIndex)
            return
        }

        guard !isFolded,
              case let .action(index)? = resolvedTarget(atX: recognizer.location(in: lens).x)
        else {
            return
        }
        onActionTapped?(index)
    }

    func commitSelection(at index: Int) {
        let isReselection = index == selectedIndex
        selectedIndex = index
        onSelect?(index, isReselection)
        if isReselection {
            updateLens(animated: true)
        }
    }

    func rebuildAccessibilityElements() {
        accessibilityElements = items.enumerated().map { index, item in
            itemAccessibilityElement(for: item, at: index)
        }
    }

    func itemAccessibilityElement(for item: DSTabBarItem, at index: Int) -> UIAccessibilityElement {
        let element = UIAccessibilityElement(accessibilityContainer: self)
        element.accessibilityLabel = item.accessibilityLabel
        element.accessibilityIdentifier = item.accessibilityIdentifier
        element.accessibilityFrameInContainerSpace = lens.convert(row.itemFrame(at: index), to: self)
        element.accessibilityTraits = [.button]

        switch item.role {
        case .tab:
            if index == selectedIndex {
                element.accessibilityTraits = [.button, .selected]
            }
        case .action:
            if #available(iOS 18.0, *) {
                element.accessibilityExpandedStatus = index == activeActionIndex ? .expanded : .collapsed
            }
        }

        return element
    }
}

extension DSTabBarView {
    func resolvedTarget(atX xPosition: CGFloat) -> Target? {
        let row = row

        let actionIndex = itemsStorage.indices.first { index in
            guard itemsStorage[index].role == .action else {
                return false
            }
            let frame = row.itemFrame(at: index)
            return xPosition >= frame.minX && xPosition <= frame.maxX
        }

        if let actionIndex {
            return .action(actionIndex)
        }

        return row.nearestItemIndex(toX: xPosition, restrictedTo: tabIndices).map(Target.tab)
    }
}
