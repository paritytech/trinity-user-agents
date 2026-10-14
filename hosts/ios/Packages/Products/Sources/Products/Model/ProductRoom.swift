import Foundation

// MARK: - Create Room

public struct CreateRoomRequest {
    public let roomId: String
    public let name: String?
    public let icon: String?

    public init(roomId: String, name: String?, icon: String?) {
        self.roomId = roomId
        self.name = name
        self.icon = icon
    }
}

public struct CreateRoomResult {
    public let status: CreateRoomStatus

    public init(status: CreateRoomStatus) {
        self.status = status
    }
}

public enum CreateRoomStatus: String {
    case new = "New"
    case exists = "Exists"
}

// MARK: - Room Info

public struct RoomInfo: Equatable {
    public let roomId: String
    public let name: String?
    public let icon: String?
    public let participation: RoomParticipation

    public init(roomId: String, name: String?, icon: String?, participation: RoomParticipation) {
        self.roomId = roomId
        self.name = name
        self.icon = icon
        self.participation = participation
    }
}

public enum RoomParticipation: String, Equatable {
    case roomHost = "RoomHost"
    case bot = "Bot"
}
