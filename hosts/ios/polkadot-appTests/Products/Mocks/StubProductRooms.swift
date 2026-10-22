import Foundation
@testable import polkadot_app

/// Answers every room with the same declarations.
struct StubProductRooms: ProductRoomReading {
    var declarations: Chat.RoomDeclarations?
    var name: String?

    func declarations(of _: Chat.Id) async -> Chat.RoomDeclarations? {
        declarations
    }

    func name(of _: Chat.Id) async -> String? {
        name
    }
}
