import Foundation

extension Chat {
    struct RoomMetadata: Equatable {
        let chatRelativeId: String
        let name: String?
        let icon: String?
        let hidesTextInput: Bool
    }
}
