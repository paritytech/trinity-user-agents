import Foundation

/// A chat extension that takes the delete of a message that the user did.
///
/// This is the seam for the Chat Extension v2 `UserDeleted` event. The bindings of today publish V1 events
/// only, and V1 has no `UserDeleted`. So the conformer cannot send the event until the V2
/// `publish_chat_event` binding of P2.10 exists.
protocol ChatUserDeletionHandling {
    func userDeleted(messageId: Chat.MessageId, roomId: String?) async
}
