import Foundation
import TrUAPIHost

/// The lookups and the events of the room screen (P6). Each one holds the worker while it runs.
extension TrUAPIChatHandler: ProductRoomInteracting {
    /// The `peer` of every event of the host user, until a room has more than one user (02-DESIGN §8.4).
    static let hostUserPeer = "native"

    func messageMenu(
        _ request: ProductChatMessageActionsQueryRequest
    ) async throws -> ProductChatMessageActionsQueryItem {
        try await withWorker { try await $0.chatMessageActionsQuery(request, timeout: ChatQueryTimeout.lookup) }
    }

    func participants(_ request: ProductChatParticipantsQueryRequest) async throws -> ProductChatParticipantsQueryItem {
        try await withWorker { try await $0.chatParticipantsQuery(request, timeout: ChatQueryTimeout.lookup) }
    }

    func participantMenu(
        _ request: ProductChatParticipantActionsQueryRequest
    ) async throws -> ProductChatParticipantActionsQueryItem {
        try await withWorker { try await $0.chatParticipantActionsQuery(request, timeout: ChatQueryTimeout.lookup) }
    }

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        try await withWorker { try await $0.chatReactionsQuery(request, timeout: ChatQueryTimeout.lookup) }
    }

    func publish(_ event: ChatEvent, roomId: String) async throws {
        try await withWorker { execution in
            try execution.publishChatEvent(roomId: roomId, peer: Self.hostUserPeer, event: event)
        }
    }
}

extension TrUAPIChatHandler: ProductRoomFooterRendering {}
