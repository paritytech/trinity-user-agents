import Foundation
import PolkadotUI
import TrUAPIHost
import UIKit

/// The participants of a product room (P6.3, 02-DESIGN §3.3.7, scenario S30): the count in the header, the author
/// of each message, and the list.
extension ChatProductRoomPresenter {
    /// True when the room asks its product for its participants (`participants`). Else the room has two
    /// participants, the user and the product.
    var asksParticipants: Bool {
        policy?.productQueries.contains(.participants) == true
    }

    /// The line under the name in the header: "N participants" when the product answered the count.
    func headerSubtitle(default subtitle: String?) -> String? {
        guard asksParticipants, let count = participants.count else {
            return subtitle
        }

        return String(localized: .ChatExtension.participantsCount(count))
    }

    /// The author of a message from another participant than the user and the product, with the name that the
    /// product resolved. The first draw asks the product, and the room draws again when the answer arrives.
    func author(of message: Chat.LocalMessage) -> ChatMessageAuthor? {
        guard asksParticipants, !message.status.isOutgoing, let participantId = message.authorParticipantId else {
            return nil
        }

        participants.resolve([participantId])

        let appearance = participants.participant(participantId).map { appearanceFactory.appearance(of: $0) }
            ?? appearanceFactory.unresolved(participantId)

        return ChatMessageAuthor(
            participantId: participantId,
            name: appearance.name,
            avatar: .colored(text: appearance.initials, colorSeed: participantId),
            onTap: { [weak self] in self?.showParticipant(participantId) }
        )
    }

    /// Starts the questions of a room that declares its participants.
    func loadParticipantsIfDeclared() {
        guard asksParticipants else { return }
        participants.loadCount()
    }

    /// The bar button that opens the list of participants, when the room declares them.
    func participantsButton() -> UIBarButtonItem? {
        guard asksParticipants else { return nil }

        let action = UIAction(
            title: String(localized: .ChatExtension.participantsButton),
            image: UIImage(systemName: "person.2")
        ) { [weak self] _ in
            self?.showParticipants()
        }
        let item = UIBarButtonItem(primaryAction: action)
        item.tintColor = .fgPrimary
        return item
    }

    func showParticipants() {
        wireframe.showParticipants(of: self, from: view)
    }
}
