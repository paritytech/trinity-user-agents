import Foundation
import TrUAPIHost
import UIKit

/// The sheet of one participant of a product room (P6.4, scenario S31).
extension ChatProductRoomPresenter {
    /// True when the room asks its product for the menu of each participant (`participant_actions`).
    var asksParticipantMenus: Bool {
        policy?.productQueries.contains(.participantActions) == true
    }

    /// The menu that the product answers for a participant, or nil for the host default.
    func participantMenu(for participant: ChatRoomParticipant) async -> Chat.ProductParticipantMenu? {
        guard asksParticipantMenus, let entry = participant.wireEntry else {
            return nil
        }

        do {
            let item = try await room.participantMenu(ProductChatParticipantActionsQueryRequest(
                roomId: roomId,
                entry: entry
            ))
            return Chat.ProductParticipantMenu(item.menu)
        } catch {
            logger.debug("Room \(roomId) uses the default participant menu: \(error)")
            return nil
        }
    }

    /// The entries of the sheet of a participant: the host entry for its kind, and the product entries.
    func participantSheetEntries(for participant: ChatRoomParticipant) async -> [ChatParticipantSheetEntry] {
        let builtIns = ChatParticipantSheet.builtIns(for: participant, productId: appearanceFactory.productId)
        let answer = await participantMenu(for: participant)
        return ChatParticipantSheet.entries(builtIns: builtIns, answer: answer)
    }

    /// In a room of the user and one other participant, the menu of that participant is the menu of the room: the
    /// header shows its entries as the native person chat shows Block and Leave (PARITY-NATIVE-IOS C15).
    func loadPeerMenu() {
        peerMenuTask?.cancel()
        guard asksParticipantMenus else {
            setPeerMenu([])
            return
        }

        peerMenuTask = Task { [weak self] in
            guard let self, let page = try? await participants.page(after: nil), !Task.isCancelled else { return }
            let others = page.participants.filter { $0.kind != .user && $0.kind != .roomProduct }
            guard page.next == nil, others.count == 1, let peer = others.first else {
                setPeerMenu([])
                return
            }

            let actions: [Chat.ProductMenuAction] =
                switch await participantMenu(for: peer) {
                case let .custom(actions)?,
                     let .extendBuiltIn(actions)?: actions
                case nil: []
                }
            guard !Task.isCancelled else { return }
            setPeerMenu(actions.inMenuOrder.map { ChatPeerMenuEntry(participant: peer, action: $0) })
        }
    }

    private func setPeerMenu(_ entries: [ChatPeerMenuEntry]) {
        guard entries != peerMenu else { return }
        peerMenu = entries
        mark("peer-menu", entries.map(\.action.actionId).joined(separator: ","))
        onHeaderChange?()
    }

    /// The entries of the menu of the peer, as entries of the header menu.
    func peerMenuActions() -> [Chat.PeerAction] {
        peerMenu.map { entry in
            .custom(Chat.CustomPeerAction(
                titleProvider: { entry.action.label },
                image: nil,
                handler: { [weak self] in self?.select(.product(entry.action), of: entry.participant) },
                destructive: entry.action.destructive
            ))
        }
    }

    /// The user picked a participant in the list or tapped the author of a message.
    func didSelectParticipant(_ participant: ChatRoomParticipant) {
        Task { [weak self] in
            guard let self else { return }
            let entries = await participantSheetEntries(for: participant)
            let appearance = appearanceFactory.appearance(of: participant)
            mark("sheet-shown", "\(participant.participantId) \(appearance.name)")
            wireframe.showParticipantSheet(
                ChatParticipantSheetViewModel(title: appearance.name, detail: appearance.detail, entries: entries),
                from: view
            ) { [weak self] entry in
                self?.select(entry, of: participant)
            }
        }
    }

    func showParticipant(_ participantId: String) {
        let participant = participants.participant(participantId)
            ?? ChatRoomParticipant(participantId: participantId, kind: .custom(nickname: participantId, avatar: nil))
        didSelectParticipant(participant)
    }

    func select(_ entry: ChatParticipantSheetEntry, of participant: ChatRoomParticipant) {
        switch entry {
        case let .copyAddress(address):
            UIPasteboard.general.string = address
        case let .openProduct(productId):
            wireframe.openProduct(productId)
        case let .product(action):
            perform(
                action.handler,
                event: .participantActionTriggered(participantId: participant.participantId, actionId: action.actionId)
            )
        }
    }
}

/// One entry of the menu of the peer of a room of two.
struct ChatPeerMenuEntry: Equatable {
    let participant: ChatRoomParticipant
    let action: Chat.ProductMenuAction
}

/// What the sheet of a participant shows.
struct ChatParticipantSheetViewModel: Equatable {
    let title: String
    let detail: String
    let entries: [ChatParticipantSheetEntry]
}
