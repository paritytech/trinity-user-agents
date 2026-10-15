import Foundation
import TrUAPIHost

/// The questions that a room screen asks the product of the room, and the events that it sends to it
/// (02-DESIGN §3.3.7 to §3.3.9, P6).
///
/// Each question throws ``ChatQueryError`` when the product refuses it, fails or does not answer in the lookup time
/// (`ChatQueryTimeout.lookup`). Then the room screen uses the host default for the item.
protocol ProductRoomInteracting: Sendable {
    func messageMenu(_ request: ProductChatMessageActionsQueryRequest) async throws
        -> ProductChatMessageActionsQueryItem

    func participants(_ request: ProductChatParticipantsQueryRequest) async throws
        -> ProductChatParticipantsQueryItem

    func participantMenu(_ request: ProductChatParticipantActionsQueryRequest) async throws
        -> ProductChatParticipantActionsQueryItem

    func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem

    /// Sends one event of the user to the product, with the peer of the host user.
    func publish(_ event: ChatEvent, roomId: String) async throws
}

/// The product of the room serves none of the questions, for example a product on the native runtime.
struct UnservedProductRoom: ProductRoomInteracting {
    struct NotServed: Error {}

    func messageMenu(_: ProductChatMessageActionsQueryRequest) async throws -> ProductChatMessageActionsQueryItem {
        throw NotServed()
    }

    func participants(_: ProductChatParticipantsQueryRequest) async throws -> ProductChatParticipantsQueryItem {
        throw NotServed()
    }

    func participantMenu(
        _: ProductChatParticipantActionsQueryRequest
    ) async throws -> ProductChatParticipantActionsQueryItem {
        throw NotServed()
    }

    func reactions(_: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
        throw NotServed()
    }

    func publish(_: ChatEvent, roomId _: String) async throws {
        throw NotServed()
    }
}

/// The custom footer of a product room (`ChatRoomFooter::Custom`, P6.6).
protocol ProductRoomFooterRendering: Sendable {
    /// The tree that the product draws for the footer of the room.
    func renderFooter(roomId: String) async -> AsyncThrowingStream<ChatRendererOutput, Error>

    /// Sends a press inside the footer to `Renderer::action_subscribe`.
    func dispatchFooterAction(roomId: String, actionId: String, payload: String?) async
}

/// The product of the room draws no footer.
struct UnservedProductRoomFooter: ProductRoomFooterRendering {
    func renderFooter(roomId _: String) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
        AsyncThrowingStream { $0.finish(throwing: UnservedProductRoom.NotServed()) }
    }

    func dispatchFooterAction(roomId _: String, actionId _: String, payload _: String?) async {}
}
