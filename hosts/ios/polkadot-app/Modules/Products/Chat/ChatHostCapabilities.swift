import Foundation

/// The Chat features that this build of the app serves from end to end, by their names in the feature table of the
/// Chat Extension v2 additions RFC (07-LATER §4.2). The core keeps a room feature that this list names, less the
/// names that it cannot back.
///
/// Add a name in the change that ships its feature, and only when the feature works end to end on the simulator.
enum ChatHostCapabilities {
    /// A user photo or video of the chat product goes to a HOP node, as the native chat sends one. The core backs
    /// it for the chat product only.
    static let hopAttachments = "hop_attachments"

    /// The header buttons and the call events (P10.7). The host draws the call with the call screens of the native
    /// chat.
    static let callButtons = "call_buttons"

    /// A room with a contact draws as a person chat of the native chat. The core backs it for the chat product only.
    static let contactRooms = "contact_rooms"

    static let served: [String] = [
        "product_source_rooms",
        "reaction_details",
        hopAttachments,
        callButtons,
        contactRooms
    ]

    static func serves(_ feature: String) -> Bool {
        served.contains(feature)
    }
}
