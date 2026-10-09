import Foundation

extension Chat {
    struct RoomMetadata: Equatable {
        let chatRelativeId: String
        let name: String?
        let icon: String?
    }

    /// What a room shows below its messages.
    enum RoomFooter: String {
        case textInput
        case empty
    }
}
