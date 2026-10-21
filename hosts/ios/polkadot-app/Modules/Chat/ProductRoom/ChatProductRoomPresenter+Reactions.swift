import Foundation
import PolkadotUI
import TrUAPIHost

/// Who reacted to a message of a product room (P6.5, 02-DESIGN §6.5, scenario S32).
extension ChatProductRoomPresenter {
    /// How a long press on the reactions of a message shows who reacted.
    enum ReactionDetailsRoute: Equatable {
        /// The host lists the reactions that it keeps.
        case localRows
        /// The host asks the product with `reactions_query`.
        case product
        /// The room does not offer who reacted.
        case none
    }

    var reactionDetailsRoute: ReactionDetailsRoute {
        guard let policy else {
            return .localRows
        }

        if policy.offersReactionDetails {
            return reactionDetailsUnavailable ? .none : .product
        }

        return policy.messageStore == .host ? .localRows : .none
    }

    /// Asks the product who reacted to a message, and shows the answer. `NotAvailable` hides the affordance for
    /// the rest of the screen. Another failure shows nothing.
    func showReactionDetails(for messageId: Chat.MessageId) {
        Task { [weak self] in
            guard let self else { return }

            do {
                let item = try await room.reactions(ProductChatReactionsQueryRequest(
                    roomId: roomId,
                    messageId: productMessageId(messageId)
                ))
                let viewModel = await reactionDetails(item.reactions, messageId: messageId)
                let groups = viewModel.reactions.map { group in
                    group.emoji + ":" + group.reactors.map(\.username).joined(separator: "+")
                }
                mark("reactions", groups.joined(separator: ","))
                view?.showReactionDetails(viewModel: viewModel)
            } catch ChatQueryError.refused(.notAvailable) {
                mark("reactions", "unavailable")
                reactionDetailsUnavailable = true
                onChange?()
            } catch {
                logger.debug("Room \(roomId) did not list who reacted: \(error)")
            }
        }
    }

    /// The reactions grouped by emoji, in the order of the product, with the names of the participants.
    func reactionDetails(
        _ reactions: [ChatReactionDetail],
        messageId: Chat.MessageId
    ) async -> ReactionDetailsViewModel {
        if asksParticipants {
            await participants.resolveNow(reactions.map(\.participantId))
        }

        let date = messages[messageId].map { Date.fromChatTimestamp($0.timestamp) } ?? Date()
        var order: [String] = []
        var groups: [String: [ReactionDetailsViewModel.Reactor]] = [:]

        for reaction in reactions {
            if groups[reaction.emoji] == nil {
                order.append(reaction.emoji)
            }
            let name = participants.participant(reaction.participantId)
                .map { appearanceFactory.appearance(of: $0).name } ?? reaction.participantId
            groups[reaction.emoji, default: []].append(
                .init(id: reaction.participantId + reaction.emoji, username: name, timestamp: date)
            )
        }

        let reactionGroups = order.map { emoji in
            ReactionDetailsViewModel.ReactionGroup(
                emoji: emoji,
                count: groups[emoji]?.count ?? 0,
                reactors: groups[emoji] ?? []
            )
        }
        return ReactionDetailsViewModel(totalCount: reactions.count, reactions: reactionGroups)
    }
}
