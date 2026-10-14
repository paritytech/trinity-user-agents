import Foundation
import TrUAPIHost

/// The participants of one product room that the room screen knows, asked from the product with
/// `participants_query` (P6.3, 02-DESIGN §3.3.7, scenario S30).
///
/// The directory keeps each participant that the product resolved, so a participant goes in one `ByIds` question
/// at most. An id that the product does not know stays unresolved. The reserved ids resolve without a question.
@MainActor
final class ChatParticipantDirectory {
    /// The page size that the host asks (02-DESIGN §3.5: 1 to 200, host default 100).
    static let pageSize: UInt32 = 100
    /// The largest number of ids in one `ByIds` question (02-DESIGN §3.5).
    static let byIdsLimit = 100

    struct Page: Equatable {
        let participants: [ChatRoomParticipant]
        let next: String?
    }

    let roomId: String
    private let room: ProductRoomInteracting
    private let logger: LoggerProtocol

    /// Called when the count or a participant arrives.
    var onChange: (() -> Void)?

    private(set) var count: Int?
    private var resolved: [String: ChatRoomParticipant] = [:]
    private var asked: Set<String> = []
    private var countTask: Task<Void, Never>?

    init(roomId: String, room: ProductRoomInteracting, logger: LoggerProtocol) {
        self.roomId = roomId
        self.room = room
        self.logger = logger
    }

    func participant(_ participantId: String) -> ChatRoomParticipant? {
        ChatRoomParticipant.reserved(participantId) ?? resolved[participantId]
    }

    /// Asks the product for the number of participants once. A refusal leaves the count unknown.
    func loadCount() {
        guard countTask == nil else { return }

        countTask = Task { [weak self, room, roomId] in
            let answer = try? await room.participants(ProductChatParticipantsQueryRequest(
                roomId: roomId,
                query: .count
            ))
            guard let self, case let .count(count) = answer else { return }
            self.count = Int(count)
            onChange?()
        }
    }

    /// Asks the product for the participants with these ids that the directory has not asked for, in batches.
    func resolve(_ participantIds: [String]) {
        let missing = Array(Set(participantIds.filter { participant($0) == nil && !asked.contains($0) })).sorted()
        guard !missing.isEmpty else { return }

        asked.formUnion(missing)
        for start in stride(from: 0, to: missing.count, by: Self.byIdsLimit) {
            let batch = Array(missing[start ..< min(start + Self.byIdsLimit, missing.count)])
            Task { [weak self] in await self?.ask(byIds: batch) }
        }
    }

    /// Asks the product for the participants with these ids that the directory does not know, and waits for the
    /// answer. A refused question leaves them unresolved.
    func resolveNow(_ participantIds: [String]) async {
        let missing = Array(Set(participantIds.filter { participant($0) == nil })).sorted()
        guard !missing.isEmpty else { return }

        asked.formUnion(missing)
        for start in stride(from: 0, to: missing.count, by: Self.byIdsLimit) {
            await ask(byIds: Array(missing[start ..< min(start + Self.byIdsLimit, missing.count)]))
        }
    }

    /// One page of the participant list, in the order of the product. A page also fills the directory.
    func page(after cursor: String?) async throws -> Page {
        let answer = try await room.participants(ProductChatParticipantsQueryRequest(
            roomId: roomId,
            query: .page(cursor: cursor, pageSize: Self.pageSize)
        ))

        guard case let .page(entries, next) = answer else {
            throw ChatQueryError.failed(reason: "the product answered another question")
        }

        let participants = entries.map(ChatRoomParticipant.init)
        store(participants)
        return Page(participants: participants, next: next)
    }
}

private extension ChatParticipantDirectory {
    func ask(byIds participantIds: [String]) async {
        do {
            let answer = try await room.participants(ProductChatParticipantsQueryRequest(
                roomId: roomId,
                query: .byIds(participantIds: participantIds)
            ))
            guard case let .resolved(entries) = answer else { return }
            store(entries.map(ChatRoomParticipant.init))
            onChange?()
        } catch {
            logger.debug("Room \(roomId) did not resolve \(participantIds.count) participants: \(error)")
        }
    }

    func store(_ participants: [ChatRoomParticipant]) {
        for participant in participants {
            resolved[participant.participantId] = participant
        }
    }
}
