import CoreData
import Foundation
import Operation_iOS

extension Chat {
    /// How long the user mutes a product room (07-LATER §6.2).
    enum MuteDuration: CaseIterable, Equatable {
        case oneHour
        case eightHours
        case oneWeek
        case always

        /// The value of `mutedUntil` for a mute that starts at `date`.
        func mutedUntil(from date: Date) -> UInt64 {
            let hour: UInt64 = 3_600_000
            let start = date.toChatTimestamp()

            switch self {
            case .oneHour: return start + hour
            case .eightHours: return start + 8 * hour
            case .oneWeek: return start + 7 * 24 * hour
            case .always: return Chat.RoomUserState.mutedForever
            }
        }
    }
}

/// One change of the user state of a product room, as the list actions and the room header menu give it.
enum ChatRoomUserStateChange: Hashable {
    case mute(Chat.MuteDuration)
    case unmute
    case archive(Bool)
}

/// Reads the mute and the archive state that the user set for a product room.
protocol ChatRoomUserStateReading: Sendable {
    /// The state of the room, or ``Chat/RoomUserState/default`` when the app does not store the room.
    func state(of chatId: Chat.Id) async throws -> Chat.RoomUserState
}

/// Changes the mute and the archive state of a product room for the user (P10.10). Each change keeps the other
/// half of the state.
protocol ChatRoomUserStateServicing: ChatRoomUserStateReading {
    func mute(_ chatId: Chat.Id, for duration: Chat.MuteDuration) async throws
    func unmute(_ chatId: Chat.Id) async throws
    func setArchived(_ archived: Bool, for chatId: Chat.Id) async throws
}

extension ChatRoomUserStateServicing {
    func apply(_ change: ChatRoomUserStateChange, to chatId: Chat.Id) async throws {
        switch change {
        case let .mute(duration): try await mute(chatId, for: duration)
        case .unmute: try await unmute(chatId)
        case let .archive(archived): try await setArchived(archived, for: chatId)
        }
    }
}

/// Reads the user state from `CDChatRoomMetadata`.
struct ChatRoomUserStateReader: ChatRoomUserStateReading {
    let storageFacade: StorageFacadeProtocol

    func state(of chatId: Chat.Id) async throws -> Chat.RoomUserState {
        let predicate = NSPredicate.chat(for: chatId.rawRepresentation)

        return try await storageFacade.databaseService.performRead { context in
            let request: NSFetchRequest<CDChat> = CDChat.fetchRequest()
            request.predicate = predicate
            request.fetchLimit = 1

            guard let metadata = try context.fetch(request).first?.roomMetadata else {
                return .default
            }
            return Chat.RoomUserState(entity: metadata)
        }
    }
}

/// Writes the user state of a product room through ``ChatRoomUserStateRecording``, which stores it and sends
/// `UserStateChanged` to the product. The list actions and the menu of the room header call it.
final class ChatRoomUserStateService: ChatRoomUserStateServicing, @unchecked Sendable {
    private let reader: ChatRoomUserStateReading
    private let recorder: ChatRoomUserStateRecording
    private let now: @Sendable () -> Date

    init(
        reader: ChatRoomUserStateReading,
        recorder: ChatRoomUserStateRecording,
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.reader = reader
        self.recorder = recorder
        self.now = now
    }

    func state(of chatId: Chat.Id) async throws -> Chat.RoomUserState {
        try await reader.state(of: chatId)
    }

    func mute(_ chatId: Chat.Id, for duration: Chat.MuteDuration) async throws {
        let current = try await reader.state(of: chatId)
        let mutedUntil = duration.mutedUntil(from: now())
        try await recorder.record(Chat.RoomUserState(mutedUntil: mutedUntil, archived: current.archived), for: chatId)
    }

    func unmute(_ chatId: Chat.Id) async throws {
        let current = try await reader.state(of: chatId)
        try await recorder.record(Chat.RoomUserState(mutedUntil: nil, archived: current.archived), for: chatId)
    }

    func setArchived(_ archived: Bool, for chatId: Chat.Id) async throws {
        let current = try await reader.state(of: chatId)
        try await recorder.record(Chat.RoomUserState(mutedUntil: current.mutedUntil, archived: archived), for: chatId)
    }
}
