import Foundation
import UIKit

/// The sheet of one participant of a product room (P6.4, scenario S31).
extension ChatProductRoomPresenter {
    /// The entries of the sheet of a participant: the host entry for its kind.
    func participantSheetEntries(for participant: ChatRoomParticipant) -> [ChatParticipantSheetEntry] {
        ChatParticipantSheet.builtIns(for: participant, productId: appearanceFactory.productId)
    }

    /// The user picked a participant in the list or tapped the author of a message.
    func didSelectParticipant(_ participant: ChatRoomParticipant) {
        let entries = participantSheetEntries(for: participant)
        let appearance = appearanceFactory.appearance(of: participant)
        mark("sheet-shown", "\(participant.participantId) \(appearance.name)")
        wireframe.showParticipantSheet(
            ChatParticipantSheetViewModel(title: appearance.name, detail: appearance.detail, entries: entries),
            from: view
        ) { [weak self] entry in
            self?.select(entry)
        }
    }

    func showParticipant(_ participantId: String) {
        let participant = participants.participant(participantId)
            ?? ChatRoomParticipant(participantId: participantId, kind: .custom(nickname: participantId, avatar: nil))
        didSelectParticipant(participant)
    }

    func select(_ entry: ChatParticipantSheetEntry) {
        switch entry {
        case let .copyAddress(address):
            UIPasteboard.general.string = address
        case let .openProduct(productId):
            wireframe.openProduct(productId)
        }
    }
}

/// What the sheet of a participant shows.
struct ChatParticipantSheetViewModel: Equatable {
    let title: String
    let detail: String
    let entries: [ChatParticipantSheetEntry]
}
