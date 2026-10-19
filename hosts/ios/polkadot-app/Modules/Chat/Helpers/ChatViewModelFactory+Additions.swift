import Foundation
import PolkadotUI
import UIKit

/// The Chat Extension v2 additions in the room screen (07-LATER §9): the thread chip, the branch control, the
/// reasoning row, the pinned root of a thread room and "Read aloud". The view model factory reads the meta of each
/// message from the product room presenter.
extension ChatViewModelFactory {
    typealias Item = IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>

    /// Draws the meta of a message: a reasoning row replaces the bubble, and the branch control and the thread chip
    /// go under it.
    func addingMeta(of message: Chat.LocalMessage, listModel: MessageListModel, to items: [Item]) -> [Item] {
        MainActor.assumeIsolated {
            guard
                !items.isEmpty,
                let productRoom,
                let meta = productRoom.meta(of: message.messageId)
            else {
                return items
            }

            if let reasoning = meta.reasoning {
                let text = listModel.getLatestEdit(for: message.messageId)?.newText ?? message.content.threadRootText
                return [reasoningItem(message, reasoning: reasoning, text: text, productRoom: productRoom)]
            }

            let side: ChatBubbleTailSide = message.status.isOutgoing ? .trailing : .leading
            var result = items
            if let branch = meta.branch {
                result.append(branchItem(message, branch: branch, side: side, productRoom: productRoom))
            }
            if let thread = meta.thread {
                result.append(threadChipItem(message, thread: thread, side: side, productRoom: productRoom))
            }
            return result
        }
    }

    /// A thread room shows its root as a pinned quote at the top, when the window holds the oldest message.
    func addingThreadRoot(to sections: [ChatViewLayout.Section], paging: ChatPagingState) -> [ChatViewLayout.Section] {
        MainActor.assumeIsolated {
            guard paging.older == .end, let root = productRoom?.threadRootConfiguration() else {
                return sections
            }

            let item = Item(id: "thread-root", configuration: SwiftUIContentConfiguration(view: root))
            guard let first = sections.first else {
                let dateText = dateSeparatorFormatter.string(for: Date())
                return [ChatViewLayout.Section(identifier: dateText, dateText: dateText, messages: [item])]
            }

            let pinned = ChatViewLayout.Section(
                identifier: first.identifier,
                dateText: first.dateText,
                messages: [item] + first.messages
            )
            return [pinned] + sections.dropFirst()
        }
    }

    /// The header of a thread room: "Thread", and the name of the parent room under it.
    @MainActor
    func threadHeaderMetadata(_ chatMetadata: ChatMetadata) -> ChatMetadata {
        guard let header = productRoom?.threadHeader() else {
            return chatMetadata
        }

        let peer = chatMetadata.peerMetadata
        return ChatMetadata(
            chatId: chatMetadata.chatId,
            peerMetadata: Chat.PeerMetadata(
                name: header.title,
                icon: peer.icon,
                input: peer.input,
                moreActions: peer.moreActions
            ),
            state: chatMetadata.state,
            subtitle: header.subtitle,
            roomPolicy: chatMetadata.roomPolicy
        )
    }

    /// The host entries of a text message, with "Read aloud" before Delete when the room declares `read_aloud`.
    func withReadAloud(_ actions: [ChatMessageBubbleAction]) -> [ChatMessageBubbleAction] {
        guard MainActor.assumeIsolated({ productRoom?.offersReadAloud == true }) else {
            return actions
        }

        var result = actions
        result.insert(.readAloud, at: result.firstIndex(of: .delete) ?? result.endIndex)
        return result
    }
}

@MainActor
private extension ChatViewModelFactory {
    func reasoningItem(
        _ message: Chat.LocalMessage,
        reasoning: Chat.Reasoning,
        text: String,
        productRoom: ChatProductRoomPresenter
    ) -> Item {
        let messageId = message.messageId
        let state: ChatReasoningRowView.State =
            if productRoom.isThinking(messageId) {
                .thinking
            } else if productRoom.isReasoningExpanded(messageId) {
                .expanded
            } else {
                .collapsed
            }

        let title =
            switch (state, reasoning.durationMs) {
            case (.thinking, _):
                String(localized: .ChatExtension.reasoningThinking)
            case let (_, durationMs?):
                String(localized: .ChatExtension.reasoningThoughtDuration(Int((Double(durationMs) / 1_000).rounded())))
            case (_, nil):
                String(localized: .ChatExtension.reasoningThought)
            }

        let accessibilityText = state == .collapsed
            ? String(localized: .ChatExtension.reasoningCollapsed)
            : String(localized: .ChatExtension.reasoningExpanded)

        let productId = productRoom.productMessageId(messageId)
        productRoom.mark("reasoning-" + productId, "\(state) \(title)")
        productRoom.mark("a11y-reasoning-" + productId, accessibilityText)

        let view = ChatReasoningRowView(
            state: state,
            title: title,
            text: text,
            accessibilityText: accessibilityText,
            copyTitle: String(localized: .chatCopy),
            onToggle: { [weak productRoom] in productRoom?.toggleReasoning(of: messageId) },
            onCopy: { UIPasteboard.general.string = text }
        )
        return Item(id: messageId, configuration: SwiftUIContentConfiguration(view: view))
    }

    func branchItem(
        _ message: Chat.LocalMessage,
        branch: Chat.BranchPosition,
        side: ChatBubbleTailSide,
        productRoom: ChatProductRoomPresenter
    ) -> Item {
        let messageId = message.messageId
        let view = ChatBranchControlView(
            index: Int(branch.index),
            count: Int(branch.count),
            pendingIndex: productRoom.pendingBranchIndex(of: messageId).map(Int.init),
            side: side,
            labels: .init(
                previous: String(localized: .ChatExtension.branchPrevious),
                next: String(localized: .ChatExtension.branchNext),
                position: String(localized: .ChatExtension.branchPosition(Int(branch.index), Int(branch.count)))
            ),
            onSelect: { [weak productRoom] index in
                productRoom?.selectBranch(of: messageId, index: UInt16(clamping: index))
            }
        )
        return Item(id: messageId + "-branch", configuration: SwiftUIContentConfiguration(view: view))
    }

    func threadChipItem(
        _ message: Chat.LocalMessage,
        thread: Chat.ThreadSummary,
        side: ChatBubbleTailSide,
        productRoom: ChatProductRoomPresenter
    ) -> Item {
        let messageId = message.messageId
        let replies = String(localized: .ChatExtension.threadReplies(Int(thread.replyCount)))
        let lastReply = thread.lastReplyAt.map { timeFormatter.string(for: Date.fromChatTimestamp($0)) }
        let text = [replies, lastReply].compactMap(\.self).joined(separator: " · ")

        let view = ChatThreadChipView(
            avatars: thread.participantIds.prefix(3).map { productRoom.threadAvatar(of: $0) },
            text: text,
            accessibilityText: String(localized: .ChatExtension.threadChipAccessibility(text)),
            side: side,
            onTap: { [weak productRoom] in productRoom?.openThread(thread, rootMessageId: messageId) }
        )
        return Item(id: messageId + "-thread", configuration: SwiftUIContentConfiguration(view: view))
    }
}

extension ChatProductRoomPresenter {
    /// The small avatar of a participant that replied in a thread: the icon of a persona, else the initial of the
    /// name that the room knows.
    func threadAvatar(of participantId: String) -> AvatarViewModel {
        if let persona = persona(of: participantId) {
            return personaAvatar(persona)
        }

        if asksParticipants {
            participants.resolve([participantId])
        }

        let appearance = participants.participant(participantId).map { appearanceFactory.appearance(of: $0) }
            ?? appearanceFactory.unresolved(participantId)
        return .colored(text: appearance.initials, colorSeed: participantId)
    }
}
