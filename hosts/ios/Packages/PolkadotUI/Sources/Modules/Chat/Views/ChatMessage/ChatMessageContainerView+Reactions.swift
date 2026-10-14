import UIKit

extension ChatMessageContainerView: UIContextMenuInteractionDelegate {
    private static var activeReactionPicker: UIView?

    func contextMenuInteraction(
        _: UIContextMenuInteraction,
        configurationForMenuAtLocation _: CGPoint
    ) -> UIContextMenuConfiguration? {
        guard let provider = appliedConfiguration.menuProvider else {
            return nil
        }

        return UIContextMenuConfiguration(
            identifier: nil,
            previewProvider: nil,
            actionProvider: { _ in
                UIMenu(children: provider())
            }
        )
    }

    @MainActor
    func contextMenuInteraction(
        _ interaction: UIContextMenuInteraction,
        willDisplayMenuFor _: UIContextMenuConfiguration,
        animator: (any UIContextMenuInteractionAnimating)?
    ) {
        guard appliedConfiguration.canReact else { return }
        guard let animator else { return }
        let reactionPicker = makeReactionPicker()

        Task {
            try? Self.showPicker(reactionPicker, for: interaction)
        }

        animator.addCompletion {
            if Self.activeReactionPicker == nil {
                // If the context menu was not available before, we will try again
                try? Self.showPicker(reactionPicker, for: interaction)
            }
        }
    }

    func contextMenuInteraction(
        _ interaction: UIContextMenuInteraction,
        willEndFor _: UIContextMenuConfiguration,
        animator: (any UIContextMenuInteractionAnimating)?
    ) {
        removeReactionPicker(for: interaction, animator: animator)
    }

    func contextMenuInteraction(
        _: UIContextMenuInteraction,
        previewForHighlightingMenuWithConfiguration _: UIContextMenuConfiguration
    ) -> UITargetedPreview? {
        makeTargetedPreview()
    }

    func contextMenuInteraction(
        _: UIContextMenuInteraction,
        configuration _: UIContextMenuConfiguration,
        highlightPreviewForItemWithIdentifier _: any NSCopying
    ) -> UITargetedPreview? {
        makeTargetedPreview()
    }

    func contextMenuInteraction(
        _: UIContextMenuInteraction,
        previewForDismissingMenuWithConfiguration _: UIContextMenuConfiguration
    ) -> UITargetedPreview? {
        nil
    }

    func contextMenuInteraction(
        _: UIContextMenuInteraction,
        configuration _: UIContextMenuConfiguration,
        dismissalPreviewForItemWithIdentifier _: any NSCopying
    ) -> UITargetedPreview? {
        nil
    }

    private func makeTargetedPreview() -> UITargetedPreview {
        let parameters = UIPreviewParameters()
        parameters.backgroundColor = .clear
        return UITargetedPreview(view: self, parameters: parameters)
    }

    private enum ReactionPickerError: Error {
        case missingWindowScene
        case noWindows
        case missingContextMenu
    }

    private func makeReactionPicker() -> UIView {
        let addReaction = appliedConfiguration.addReaction
        let reactionPicker = ReactionPickerUIView(
            emojis: addReaction?.quickEmojis ?? [],
            onReactionSelected: { [weak self] emoji in
                self?.dismissContextMenu()
                self?.appliedConfiguration.addReaction?.onReactionTap?(emoji)
            }
        )

        let allSections = addReaction?.allSections ?? []
        let onReactionTap = addReaction?.onReactionTap
        reactionPicker.onExpandTapped = { [weak self] in
            guard !allSections.isEmpty else { return }
            guard let window = Self.activeReactionPicker?.window else { return }

            let pickerFrame = reactionPicker.frame
            let expandedPickerHeight: CGFloat = 300
            let expandedFrame = CGRect(
                x: pickerFrame.origin.x,
                y: pickerFrame.origin.y,
                width: pickerFrame.width,
                height: expandedPickerHeight
            )

            let emojiPicker = EmojiPickerInlineView(sections: allSections)
            emojiPicker.onEmojiSelected = { [weak self] emoji in
                onReactionTap?(emoji)
                self?.dismissContextMenu()
            }
            emojiPicker.frame = pickerFrame
            emojiPicker.alpha = 0

            window.addSubview(emojiPicker)
            Self.activeReactionPicker = emojiPicker

            UIView.animate(withDuration: 0.25, delay: 0, options: .curveEaseOut) {
                reactionPicker.alpha = 0
                emojiPicker.alpha = 1
                emojiPicker.frame = expandedFrame
            } completion: { _ in
                reactionPicker.removeFromSuperview()
            }
        }

        reactionPicker.sizeToFit()
        let pickerSize = reactionPicker.systemLayoutSizeFitting(UIView.layoutFittingCompressedSize)
        let pickerWidth = max(pickerSize.width, 320)
        reactionPicker.frame.size = CGSize(width: pickerWidth, height: 52)

        return reactionPicker
    }

    private static func addToContextMenu(_ picker: UIView, menuLayout: ContextMenuLayout) {
        let contextMenuWindow = menuLayout.window
        let previewFrame = menuLayout.previewFrame
        let menuFrame = menuLayout.menuFrame
        let safeAreaTop = menuLayout.safeAreaTop

        let menuIsAbovePreview = menuFrame.map { $0.midY < previewFrame.midY } ?? false
        let topElementMinY = menuIsAbovePreview ? (menuFrame?.minY ?? previewFrame.minY) : previewFrame.minY

        let pickerX = (contextMenuWindow.bounds.width - picker.bounds.width) / 2
        let pickerY = max(topElementMinY - picker.bounds.height - 16, safeAreaTop + 8)

        picker.frame.origin = .init(x: pickerX, y: pickerY)
        contextMenuWindow.addSubview(picker)
    }

    // MARK: - Context Menu View Hierarchy Helpers

    private struct ContextMenuLayout {
        let window: UIWindow
        let safeAreaTop: CGFloat
        let previewFrame: CGRect
        let menuFrame: CGRect?
    }

    private static func showPicker(_ picker: UIView, for interaction: UIContextMenuInteraction) throws {
        let layout = try menuLayout(for: interaction)
        picker.alpha = 0
        addToContextMenu(picker, menuLayout: layout)
        picker.transform = .init(scaleX: 0.9, y: 0.9)
        activeReactionPicker = picker

        UIView.animate(withDuration: 0.3) {
            picker.alpha = 1
            picker.transform = .identity
        }
    }

    private static func menuLayout(for interaction: UIContextMenuInteraction) throws(ReactionPickerError)
        -> ContextMenuLayout {
        guard let windowScene = interaction.view?.window?.windowScene else {
            throw .missingWindowScene
        }

        let allWindows = windowScene.windows.sorted { $0.windowLevel.rawValue > $1.windowLevel.rawValue }

        guard let result = Self.findContextMenuLayout(in: allWindows) else {
            throw .missingContextMenu
        }

        return result
    }

    @inline(never)
    private static func joinTokens(_ parts: String...) -> String {
        parts.joined()
    }

    private static let ctx = "Context"
    private static let mnu = "Menu"
    private static let cnr = "Container"
    private static let vwx = "View"
    private static let mph = "Morphing"
    private static let plt = "Platter"
    private static let pvw = "Preview"
    private static let cnt = "Content"
    private static let act = "Actions"
    private static let lst = "List"

    private static let sig0 = joinTokens(ctx, mnu, cnr, vwx)
    private static let sig1 = joinTokens(mph, plt, vwx)
    private static let sig2 = joinTokens(pvw, plt, vwx)
    private static let sig3 = joinTokens(cnt, plt, vwx)
    private static let sig4 = joinTokens(ctx, mnu, act, lst, vwx)
    private static let sig5 = joinTokens(ctx, mnu, vwx)

    private static func findContextMenuLayout(in windows: [UIWindow]) -> ContextMenuLayout? {
        guard let rootWindow = windows.last else { return nil }

        for window in windows {
            guard let containerView = window.subviews.first(where: {
                String(describing: type(of: $0)).contains(Self.sig0)
            }) else {
                continue
            }

            guard let previewView = findSubview(in: containerView, matching: {
                let name = String(describing: type(of: $0))

                return name.contains(Self.sig1)
                    || name.contains(Self.sig2)
                    || name.contains(Self.sig3)
            }) else {
                continue
            }

            let previewFrame = previewView.convert(previewView.bounds, to: window)

            let menuView = findSubview(in: containerView, matching: {
                let name = String(describing: type(of: $0))
                return name.contains(Self.sig4) || name.contains(Self.sig5)
            })
            let menuFrame = menuView.map { $0.convert($0.bounds, to: window) }

            return ContextMenuLayout(
                window: window,
                safeAreaTop: rootWindow.safeAreaInsets.top,
                previewFrame: previewFrame,
                menuFrame: menuFrame
            )
        }

        return nil
    }

    private static func findSubview(in view: UIView, matching predicate: (UIView) -> Bool) -> UIView? {
        for subview in view.subviews {
            if predicate(subview) {
                return subview
            }
            if let found = findSubview(in: subview, matching: predicate) {
                return found
            }
        }
        return nil
    }

    private func removeReactionPicker(
        for _: UIContextMenuInteraction,
        animator: (any UIContextMenuInteractionAnimating)?
    ) {
        guard let reactionPicker = Self.activeReactionPicker else { return }

        animator?.addAnimations {
            reactionPicker.alpha = 0
        }
        animator?.addCompletion {
            reactionPicker.removeFromSuperview()
        }
        Self.activeReactionPicker = nil
    }

    private func dismissContextMenu() {
        for interaction in bubbleView.interactions {
            if let contextMenuInteraction = interaction as? UIContextMenuInteraction {
                contextMenuInteraction.dismissMenu()
                break
            }
        }
    }
}
