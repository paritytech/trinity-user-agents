import Foundation
import SwiftUI
import UIKit
import DesignSystem
internal import SnapKit

public final class ChatViewLayout: DiffableCollectionViewProviderView<String, String> {
    public let centerItem = ChatHeaderView(configuration: .empty())

    public var contactHeaderConfiguration: ChatHeaderConfiguration {
        centerItem.appliedConfiguration
    }

    private let chatInputHostView = ChatInputHostView()
    private let scrollDownButton = ScrollToBottomButton()
    private let scrollToReactionButton = ScrollToReactionButton()
    private var currentInputViewHeight: CGFloat = 0
    private var currentKeyboardHeight: CGFloat = 0

    let dayHeaderCellIdentifier = "ChatDayHeaderReuseIdentifier"

    private static let chatDayHeaderElementKind = "ChatDayHeaderElement"
    private static let footerSectionIdentifier = "FooterSection"
    private static let footerCellIdentifier = "ChatFooterCell"

    private var currentFooterConfiguration: (any HashableContentConfiguration)?

    private var replyToMessageId: String?
    private var editingMessageId: String?

    public var onTransferTap: (() -> Void)?
    public var onAttachmentTap: (() -> Void)?
    public var onRemoveAttachmentTap: ((_ index: Int) -> Void)?
    public var onSendTap: ((String, String?) -> Void)?
    public var onEditSendTap: ((_ messageId: String, _ newText: String) -> Void)?
    public var onReplyMessage: ((_ identifier: String) -> Void)?
    public var onScrollToBottomTap: (() -> Void)?
    public var onScrollToReactionTap: (() -> Void)?

    var viewModel: ViewModel?

    private var dayHeaderTexts = [String]()

    private var isInitialScrollDone = false
    private var isAutoScrolling = false
    private var pendingSendScroll = false
    private var scrollDownButtonVisible = false
    private var scrollToReactionButtonVisible = false
    private var dismissedReactionTargetId: String?

    override public func baseSetup() {
        dataSource.supplementaryViewProvider = { [weak self] collectionView, kind, indexPath in
            self?.provideSupplementaryView(for: collectionView, indexPath: indexPath, kind: kind)
        }
        super.baseSetup()
    }

    override public func setupViews() {
        backgroundColor = .bgSurfaceMain
        collectionView.backgroundColor = .clear
        collectionView.keyboardDismissMode = .interactive
        collectionView.delegate = self

        let tapGesture = UITapGestureRecognizer(target: self, action: #selector(dismissKeyboard))
        tapGesture.cancelsTouchesInView = false
        collectionView.addGestureRecognizer(tapGesture)

        addSubview(collectionView)
        collectionView.snp.makeConstraints {
            $0.edges.equalToSuperview()
        }

        addSubview(scrollToReactionButton)
        scrollToReactionButton.alpha = 0
        scrollToReactionButton.addTarget(self, action: #selector(handleScrollToReaction), for: .touchUpInside)

        addSubview(scrollDownButton)
        scrollDownButton.alpha = 0
        scrollDownButton.addTarget(self, action: #selector(handleScrollToBottom), for: .touchUpInside)
        scrollDownButton.snp.makeConstraints { make in
            make.trailing.equalToSuperview().inset(16)
            make.bottom.equalTo(safeAreaLayoutGuide.snp.bottom)
        }

        scrollToReactionButton.snp.makeConstraints { make in
            make.trailing.equalTo(scrollDownButton)
            make.bottom.equalTo(scrollDownButton.snp.top).offset(-7)
        }

        setupScrollEdgeEffect()
    }

    private func setupScrollEdgeEffect() {
        // iOS 26: the system fades chat content scrolling beneath the floating input bar
        // (same soft scroll edge effect as the Liquid Glass tab bar). The bar's glass
        // views/controls automatically shape the effect.
        guard #available(iOS 26.0, *) else { return }
        let interaction = UIScrollEdgeElementContainerInteraction()
        interaction.scrollView = collectionView
        interaction.edge = .bottom
        chatInputHostView.addInteraction(interaction)
    }

    override public func registerCells() {
        super.registerCells()

        [
            ChatMessageTextView.reuseIdentifier,
            ChatMessageSingleEmojiView.reuseIdentifier,
            ChatMessageTextImageCell.reuseIdentifier,
            ChatMessageFileCell.reuseIdentifier,
            ChatInfoMessageConfiguration.defaultReuseIdentifier,
            ChatTransferMessageConfiguration.defaultReuseIdentifier,
            ChatCallMessageConfiguration.defaultReuseIdentifier,
            ChatSystemMessageConfiguration.defaultReuseIdentifier,
            ChatMessageMediaViewConfiguration.defaultReuseIdentifier,
            ChatRichTextMessageConfiguration.reuseIdentifier,
            SwiftUIContentConfiguration.defaultReuseIdentifier,
        ]
        .forEach {
            CollectionRegistration.registerCell(
                UICollectionViewCell.self,
                for: collectionView,
                reuseId: $0
            )
        }

        CollectionRegistration.registerCell(
            UICollectionViewCell.self,
            for: collectionView,
            reuseId: Self.footerCellIdentifier
        )

        collectionView.register(
            UICollectionViewCell.self,
            forSupplementaryViewOfKind: ChatViewLayout.chatDayHeaderElementKind,
            withReuseIdentifier: dayHeaderCellIdentifier
        )
    }

    override public func createLayout() -> UICollectionViewCompositionalLayout {
        let sectionProvider: UICollectionViewCompositionalLayoutSectionProvider =
            { [unowned self] section, environment in
                let provider = sectionProviders[section].sectionLayoutProvider
                return provider(section, environment)
            }

        return UICollectionViewCompositionalLayout(sectionProvider: sectionProvider)
    }
}

// MARK: - Chat input

extension ChatViewLayout {
    func attachInputHostIfNeeded() {
        guard chatInputHostView.superview == nil else {
            return
        }

        addSubview(chatInputHostView)
        chatInputHostView.snp.makeConstraints {
            $0.leading.trailing.equalToSuperview()
            $0.bottom.equalToSuperview()
        }

        scrollDownButton.snp.remakeConstraints { make in
            make.trailing.equalToSuperview().inset(16)
            make.bottom.equalTo(chatInputHostView.snp.top).offset(-16)
        }
    }

    func hideInput() {
        chatInputHostView.removeFromSuperview()
        scrollDownButton.snp.remakeConstraints { make in
            make.trailing.equalToSuperview().inset(16)
            make.bottom.equalTo(safeAreaLayoutGuide.snp.bottom).offset(-16)
        }
        updateContentInsets(bottomInset: currentKeyboardHeight)
    }
}

// MARK: - Public interface

public extension ChatViewLayout {
    // General model binding
    func bind(viewModel: ViewModel) {
        self.viewModel = viewModel

        if let inputConfiguration = viewModel.chatInputConfiguration {
            attachInputHostIfNeeded()

            chatInputHostView.bind(
                configuration: inputConfiguration,
                inputHandler: self,
                keyboardGuide: keyboardLayoutGuide
            )

            currentInputViewHeight = chatInputHostView.contentHeight
        } else {
            hideInput()
            chatInputHostView.clear()
            currentInputViewHeight = 0
        }
        layoutIfNeeded()
        updateContentInsets(bottomInset: currentKeyboardHeight)

        scrollDownButton.unreadCount = viewModel.scrollDownConfiguration.unreadCount

        centerItem.configuration = viewModel.headerConfiguration

        updateView(
            sections: viewModel.sections,
            footer: viewModel.footerConfiguration
        )

        updateScrollToReactionButtonVisibility()
    }

    // Use for footer updates optimization
    func setFooter(_ footer: (any HashableContentConfiguration)?) {
        let shouldAutoScroll = !isInitialScrollDone || isAtBottom()

        let shouldUpdate: Bool =
            switch (footer, currentFooterConfiguration) {
            case (nil, nil):
                false
            case (nil, .some),
                 (.some, nil):
                true
            case let (.some(lhs), .some(rhs)):
                AnyHashable(lhs) != AnyHashable(rhs)
            }

        guard shouldUpdate else { return }

        currentFooterConfiguration = footer

        if let footer {
            let footerSection = makeFooterSection(with: footer)
            if sectionProviders.last?.id == Self.footerSectionIdentifier {
                updateSectionData(section: footerSection)
            } else {
                appendSection(footerSection)
            }
        } else {
            removeSection(id: Self.footerSectionIdentifier)
        }

        guard shouldAutoScroll else { return }
        scrollToBottom(animated: isInitialScrollDone)
        isInitialScrollDone = true
    }
}

// MARK: - KeyboardAdoptableViewLayout

public extension ChatViewLayout {
    var activatesInputOnAppear: Bool {
        viewModel?.chatInputConfiguration?.activateOnAppear == true
    }

    func focusInput() {
        chatInputHostView.textInputInterface?.activateTextField()
    }

    func adoptToVisibleKeyboard(bottomInset: CGFloat) {
        updateContentInsets(bottomInset: bottomInset)
    }

    func adoptToHiddenKeyboard() {
        updateContentInsets(bottomInset: 0)
    }

    private func updateContentInsets(bottomInset newHeight: CGFloat) {
        currentKeyboardHeight = newHeight

        let currentOffset = collectionView.contentOffset
        let oldContentInsetBottom = collectionView.contentInset.bottom
        let oldAdjustedBottomInset = collectionView.adjustedContentInset.bottom
        let adjustedTopInset = collectionView.adjustedContentInset.top

        let newContentInsetBottom = currentKeyboardHeight > 0 ? currentKeyboardHeight + currentInputViewHeight :
            currentInputViewHeight
        let insetChange = newContentInsetBottom - oldContentInsetBottom

        collectionView.contentInset.bottom = newContentInsetBottom
        collectionView.verticalScrollIndicatorInsets.bottom = newContentInsetBottom

        let oldAvailableHeight = collectionView.bounds.height - adjustedTopInset - oldAdjustedBottomInset
        let newAvailableHeight = collectionView.bounds.height - adjustedTopInset - newContentInsetBottom

        let contentHeight = collectionView.contentSize.height

        // Check if content fit before but doesn't fit after
        let contentFitBefore = contentHeight <= oldAvailableHeight
        let contentFitsAfter = contentHeight <= newAvailableHeight

        if contentFitBefore, !contentFitsAfter {
            // Scroll to bottom
            let newOffset = contentHeight - adjustedTopInset - newAvailableHeight
            collectionView.contentOffset.y = newOffset
        } else if contentFitsAfter {
            // Scroll to top
            collectionView.contentOffset.y = -adjustedTopInset
        } else if !contentFitBefore {
            // Maintain visual position by adjusting offset by the inset change
            let newOffset = currentOffset.y + insetChange
            collectionView.contentOffset.y = max(-adjustedTopInset, newOffset)
        }

        // The input bar tracks the keyboard via its `keyboardGuide.snp.top` pin, which UIKit
        // already animates inside the keyboard transition.
        layoutIfNeeded()
    }
}

private extension ChatViewLayout {
    /// Updates the view with new content
    /// Scroll behavior:
    /// - On initial load (!isInitialScrollDone): scroll to firstUnreadMessage or bottom
    /// - On subsequent updates: only auto-scroll if user is already at bottom or just sent a message
    /// - If user has scrolled away: maintain their position
    func updateView(
        sections: [Section],
        footer: (any HashableContentConfiguration)?
    ) {
        let autoScroll = makeAutoScrollResult()

        updateSectionProviders(
            sections: sections,
            footer: footer,
            completion: autoScroll.closureForUpdateScrolling
        )

        autoScroll.closureForInitialScrolling?()
    }

    func updateSectionProviders(
        sections: [Section],
        footer: (any HashableContentConfiguration)?,
        completion: (() -> Void)? = nil
    ) {
        var providers = [SectionProviderType]()
        var headerTexts = [String]()

        for section in sections {
            headerTexts.append(section.dateText)

            var itemProviders = [ItemProviderType]()

            itemProviders.append(contentsOf: section.messages.map {
                ItemProviderType(
                    id: $0.id,
                    configuration: $0.configuration,
                    reuseIdentifier: ($0.configuration as? ChatMessageContainerConfiguration)?.identifier ?? $0
                        .configuration.defaultReuseIdentifier
                )
            })

            let layoutSection = layoutSection()
            addDayHeader(to: layoutSection)

            let providerType = SectionProviderType(
                id: section.identifier,
                itemProviders: itemProviders
            ) { _, _ in layoutSection }

            providers.append(providerType)
        }

        if let footer {
            let footerSection = makeFooterSection(with: footer)
            providers.append(footerSection)
        }

        currentFooterConfiguration = footer
        dayHeaderTexts = headerTexts
        applySnapshot(sections: providers, completion: completion)
    }

    func layoutSection() -> NSCollectionLayoutSection {
        let section = NSCollectionLayoutSection(group: .list(
            heightDimension: .estimated(60),
            widthDimension: .fractionalWidth(1.0)
        ))
        section.interGroupSpacing = DSSpacings.small
        section.contentInsets = .init(top: 10, leading: 16, bottom: 10, trailing: 16)
        return section
    }

    func addDayHeader(to section: NSCollectionLayoutSection) {
        let headerSize = NSCollectionLayoutSize(
            widthDimension: .fractionalWidth(1.0),
            heightDimension: .estimated(36)
        )
        let header = NSCollectionLayoutBoundarySupplementaryItem(
            layoutSize: headerSize,
            elementKind: Self.chatDayHeaderElementKind,
            alignment: .top
        )
        header.pinToVisibleBounds = true
        header.zIndex = 2
        section.boundarySupplementaryItems = [header]
    }

    func makeFooterSection(with configuration: any HashableContentConfiguration) -> SectionProviderType {
        let itemProvider = ItemProviderType(
            id: Self.footerCellIdentifier,
            configuration: configuration,
            reuseIdentifier: Self.footerCellIdentifier
        )
        let footerSection = SectionProviderType(
            id: Self.footerSectionIdentifier,
            itemProviders: [itemProvider]
        ) { _, _ in
            let section = NSCollectionLayoutSection.autoHeightSingleItem(100)
            section.contentInsets = .zero
            return section
        }
        return footerSection
    }

    // MARK: - Scrolling Helpers

    private struct AutoScrollResult {
        let closure: (() -> Void)?
        let isInitialScrolling: Bool

        var closureForInitialScrolling: (() -> Void)? {
            isInitialScrolling ? closure : nil
        }

        var closureForUpdateScrolling: (() -> Void)? {
            isInitialScrolling ? nil : closure
        }
    }

    private func makeAutoScrollResult() -> AutoScrollResult {
        let shouldScroll = !isInitialScrollDone || pendingSendScroll || isAtBottom()
        pendingSendScroll = false

        guard shouldScroll else {
            return AutoScrollResult(
                closure: nil,
                isInitialScrolling: false
            )
        }

        let animated = isInitialScrollDone

        let closure: () -> Void = { [weak self] in
            guard let self else { return }
            performScrollToBottom(animated: animated)
            isInitialScrollDone = true
            updateScrollDownButtonVisibility()
        }

        return AutoScrollResult(
            closure: closure,
            isInitialScrolling: !isInitialScrollDone
        )
    }

    private func performScrollToBottom(animated: Bool) {
        // On initial scroll: prioritize firstUnreadMessageIdentifier
        if !isInitialScrollDone,
           let firstUnreadMessageIdentifier = viewModel?.firstUnreadMessageIdentifier {
            scrollTo(itemIdentifier: firstUnreadMessageIdentifier, animated: animated)
            return
        }

        // Otherwise, scroll to the very bottom (including footer)
        scrollToBottom(animated: animated)
    }

    private func isAtBottom() -> Bool {
        let lastSection = collectionView.numberOfSections - 1
        guard lastSection >= 0 else { return true }
        let lastItem = collectionView.numberOfItems(inSection: lastSection) - 1
        guard lastItem >= 0 else { return true }

        let lastIndexPath = IndexPath(item: lastItem, section: lastSection)
        guard let lastAttributes = collectionView.layoutAttributesForItem(at: lastIndexPath) else {
            return true
        }

        let maxHiddenPart: CGFloat = 40

        let visibleBottom = collectionView.bounds.maxY - collectionView.adjustedContentInset.bottom
        let hiddenPart = lastAttributes.frame.maxY - visibleBottom

        return hiddenPart < maxHiddenPart
    }

    private func scrollToBottom(animated: Bool) {
        let snapshot = dataSource.snapshot()
        guard let lastSection = snapshot.sectionIdentifiers.last,
              let lastItem = snapshot.itemIdentifiers(inSection: lastSection).last,
              let lastIndexPath = dataSource.indexPath(for: lastItem) else {
            return
        }
        if animated {
            isAutoScrolling = true
        }
        collectionView.scrollToItem(at: lastIndexPath, at: .bottom, animated: animated)
    }

    func scrollTo(
        itemIdentifier: ItemIdentifierType,
        animated: Bool,
        position: UICollectionView.ScrollPosition = .top
    ) {
        guard let indexPath = dataSource.indexPath(for: itemIdentifier) else { return }
        if animated {
            isAutoScrolling = true
        }
        collectionView.scrollToItem(at: indexPath, at: position, animated: animated)
    }

    private func updateScrollDownButtonVisibility() {
        guard !isAutoScrolling else { return }
        let shouldBeVisible = (viewModel?.scrollDownConfiguration.available ?? false) && !isAtBottom()
        guard scrollDownButtonVisible != shouldBeVisible else { return }

        scrollDownButtonVisible = shouldBeVisible
        UIView.animate(withDuration: 0.2) {
            self.scrollDownButton.alpha = shouldBeVisible ? 1 : 0
        }
    }

    private func updateScrollToReactionButtonVisibility() {
        let shouldBeVisible: Bool = {
            guard let targetId = viewModel?.scrollToReactionConfiguration.targetMessageId else {
                dismissedReactionTargetId = nil
                return false
            }
            if targetId == dismissedReactionTargetId {
                return false
            }
            return !visibleIdentifiers().contains(targetId)
        }()
        guard scrollToReactionButtonVisible != shouldBeVisible else { return }

        scrollToReactionButtonVisible = shouldBeVisible
        UIView.animate(withDuration: 0.2) {
            self.scrollToReactionButton.alpha = shouldBeVisible ? 1 : 0
        }
    }

    private func dayHeaderMargins() -> EdgeInsets {
        EdgeInsets(top: DSSpacings.extraSmall, leading: 0, bottom: 0, trailing: 0)
    }

    // MARK: - Actions

    @objc
    private func dismissKeyboard() {
        endEditing(true)
    }

    @objc
    private func handleScrollToBottom() {
        performScrollToBottom(animated: true)
        onScrollToBottomTap?()
    }

    @objc
    private func handleScrollToReaction() {
        guard let targetMessageId = viewModel?.scrollToReactionConfiguration.targetMessageId else {
            return
        }
        dismissedReactionTargetId = targetMessageId
        scrollTo(itemIdentifier: targetMessageId, animated: true, position: .centeredVertically)

        scrollToReactionButtonVisible = false
        UIView.animate(withDuration: 0.2) {
            self.scrollToReactionButton.alpha = 0
        }

        onScrollToReactionTap?()
    }

    // MARK: - Reply Helpers

    func showReplyBanner(title: String, messageText: String) {
        chatInputHostView.replyInterface?.showReplyBanner(title: title, messageText: messageText)
    }

    func hideReplyBanner() {
        chatInputHostView.replyInterface?.hideReplyBanner()
    }

    func clearReplyContext() {
        replyToMessageId = nil
        hideReplyBanner()
    }

    func showEditBanner(title: String, currentText: String) {
        chatInputHostView.editInterface?.showEditBanner(title: title, currentText: currentText)
    }

    func hideEditBanner() {
        chatInputHostView.editInterface?.hideEditBanner()
    }

    func clearEditContext() {
        editingMessageId = nil
        hideEditBanner()
    }

    func focusTextField() {
        chatInputHostView.textInputInterface?.activateTextField()
    }

    // MARK: - Supplementary views

    func provideSupplementaryView(
        for collectionView: UICollectionView,
        indexPath: IndexPath,
        kind: String
    ) -> UICollectionReusableView? {
        makeDayHeaderSupplementaryView(
            collectionView: collectionView,
            indexPath: indexPath,
            kind: kind
        )
    }

    func makeDayHeaderSupplementaryView(
        collectionView: UICollectionView,
        indexPath: IndexPath,
        kind: String
    ) -> UICollectionReusableView? {
        guard kind == ChatViewLayout.chatDayHeaderElementKind else {
            return nil
        }

        let headerIndex = indexPath.section
        guard headerIndex >= 0, headerIndex < dayHeaderTexts.count else {
            return nil
        }

        guard let supplementaryView = collectionView.dequeueReusableSupplementaryView(
            ofKind: kind,
            withReuseIdentifier: dayHeaderCellIdentifier,
            for: indexPath
        ) as? UICollectionViewCell else {
            assertionFailure("Cannot dequeue supplementary view of type UICollectionViewCell")
            return nil
        }

        let view = DSChatMarker(text: dayHeaderTexts[headerIndex], style: .list)
        let configuration = SwiftUIContentConfiguration(view: view, margins: dayHeaderMargins())
        supplementaryView.contentConfiguration = configuration

        return supplementaryView
    }
}

public extension ChatViewLayout {
    func activateReply(messageId: String, username: String, text: String) {
        clearEditContext()

        replyToMessageId = messageId
        let bannerTitle = String(localized: .chatReplyTo(username))

        showReplyBanner(title: bannerTitle, messageText: text)
        focusTextField()
    }

    func activateEdit(messageId: String, currentText: String) {
        clearReplyContext()

        editingMessageId = messageId
        let bannerTitle = String(localized: .chatEditMessage)

        showEditBanner(title: bannerTitle, currentText: currentText)
        focusTextField()
    }
}

// MARK: - UICollectionViewDelegate

extension ChatViewLayout: UICollectionViewDelegate {
    public func scrollViewDidScroll(_ scrollView: UIScrollView) {
        updateScrollDownButtonVisibility()
        updateScrollToReactionButtonVisibility()
        updateDayHeadersPinnedState(insideOf: scrollView)
    }

    public func scrollViewDidEndScrollingAnimation(_: UIScrollView) {
        isAutoScrolling = false
        updateScrollDownButtonVisibility()
    }

    public func scrollViewWillBeginDragging(_: UIScrollView) {
        isAutoScrolling = false
    }

    public func collectionView(
        _: UICollectionView,
        willDisplay cell: UICollectionViewCell,
        forItemAt indexPath: IndexPath
    ) {
        guard let identifier = dataSource.itemIdentifier(for: indexPath) else {
            return
        }

        if let containerView = cell.contentView as? ChatMessageContainerView {
            containerView.onReply = { [weak self] in
                self?.onReplyMessage?(identifier)
            }
            containerView.onQuoteTap = { [weak self] messageId in
                self?.scrollTo(itemIdentifier: messageId, animated: true, position: .centeredVertically)
            }
        }
    }
}

// MARK: - ChatInputHandling

extension ChatViewLayout: ChatInputHandling {
    public func chatInputDidChange() {
        let newHeight = chatInputHostView.contentHeight
        guard newHeight != currentInputViewHeight else {
            return
        }

        currentInputViewHeight = newHeight
        updateContentInsets(bottomInset: currentKeyboardHeight)
    }

    public func chatInputDidSend(_ text: String) {
        if let editingMessageId {
            onEditSendTap?(editingMessageId, text)
            clearEditContext()
        } else {
            pendingSendScroll = true
            onSendTap?(text, replyToMessageId)
            clearReplyContext()
        }
    }

    public func chatInputDidTransfer() {
        onTransferTap?()
    }

    public func chatInputDidAttachment() {
        onAttachmentTap?()
    }

    public func chatInputDidCancelReply() {
        clearReplyContext()
    }

    public func chatInputDidCancelEdit() {
        clearEditContext()
    }
}

// MARK: - Contact header

extension ChatViewLayout {
    private func updateDayHeadersPinnedState(insideOf scrollView: UIScrollView) {
        guard let collectionView = scrollView as? UICollectionView else {
            return
        }

        let topInset = collectionView.adjustedContentInset.top
        let pinY = collectionView.contentOffset.y + topInset
        let tolerance: CGFloat = 1.0

        let visibleIndexPaths = collectionView.indexPathsForVisibleSupplementaryElements(
            ofKind: Self.chatDayHeaderElementKind
        )

        var wasSetAsPinned = false

        for indexPath in visibleIndexPaths {
            guard let header = collectionView.supplementaryView(
                forElementKind: Self.chatDayHeaderElementKind,
                at: indexPath
            ) as? UICollectionViewCell else {
                continue
            }

            let headerIndex = indexPath.section
            guard headerIndex >= 0, headerIndex < dayHeaderTexts.count else {
                continue
            }

            let style: DSChatMarker.Style
            if wasSetAsPinned {
                style = .list
            } else if let attrs = collectionView.collectionViewLayout.layoutAttributesForSupplementaryView(
                ofKind: Self.chatDayHeaderElementKind,
                at: indexPath
            ), abs(attrs.frame.minY - pinY) < tolerance {
                style = .floated
                wasSetAsPinned = true
            } else {
                style = .list
            }

            let view = DSChatMarker(text: dayHeaderTexts[headerIndex], style: style)
            let configuration = SwiftUIContentConfiguration(view: view, margins: dayHeaderMargins())
            header.contentConfiguration = configuration
        }
    }

    public func visibleIdentifiers() -> [ItemIdentifierType] {
        let bounds = collectionView.bounds
        let inset = collectionView.adjustedContentInset
        let visibleBounds = bounds.inset(by: inset)

        guard let attributes = collectionView.collectionViewLayout.layoutAttributesForElements(in: visibleBounds) else {
            return []
        }

        return attributes.compactMap { attr in
            guard attr.representedElementCategory == .cell else {
                return nil
            }
            // Exclude footer
            let identifier = dataSource.itemIdentifier(for: attr.indexPath)
            guard identifier != Self.footerCellIdentifier else {
                return nil
            }
            return identifier
        }
    }
}
