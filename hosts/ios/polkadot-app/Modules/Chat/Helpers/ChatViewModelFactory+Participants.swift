import Foundation
import PolkadotUI

/// The participants of a product room in the room screen (P6.3).
extension ChatViewModelFactory {
    /// The metadata of the header: in a room that declares participants, "N participants" replaces the
    /// description once the product answers the count.
    @MainActor
    func productRoomHeaderMetadata(_ chatMetadata: ChatMetadata) -> ChatMetadata {
        guard let productRoom else {
            return chatMetadata
        }

        var metadata = chatMetadata
        metadata.subtitle = productRoom.headerSubtitle(default: chatMetadata.subtitle)
        return metadata
    }

    /// Draws the author above the bubble of a message from another participant than the user and the product.
    func addingAuthor(
        of message: Chat.LocalMessage,
        to configurations: [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>]
    ) -> [IdentifiableAnyContentConfiguration<ChatViewLayout.ItemIdentifierType>] {
        guard
            message.authorParticipantId != nil,
            let author = MainActor.assumeIsolated({ productRoom?.author(of: message) }),
            let first = configurations.first,
            var container = first.contentConfiguration as? ChatMessageContainerConfiguration
        else {
            return configurations
        }

        container.setAuthor(author)
        return [IdentifiableAnyContentConfiguration(id: first.id, configuration: container)]
            + configurations.dropFirst()
    }
}
