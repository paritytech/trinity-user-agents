import AsyncExtensions
import Foundation
@testable import polkadot_app

/// Personas that a test pushes to the room screen.
final class PushedPersonaStore: ChatProductPersonaStoring, @unchecked Sendable {
    private let stream: AsyncStream<[Chat.ProductPersona]>
    private let continuation: AsyncStream<[Chat.ProductPersona]>.Continuation

    init() {
        (stream, continuation) = AsyncStream<[Chat.ProductPersona]>.makeStream()
    }

    func push(_ personas: [Chat.ProductPersona]) {
        continuation.yield(personas)
    }

    func register(_: Chat.ProductPersona) async throws -> Chat.PersonaRegistration {
        .new
    }

    func personas(of _: String) async throws -> [Chat.ProductPersona] {
        []
    }

    func personaUpdates(of _: String) -> AnyAsyncSequence<[Chat.ProductPersona]> {
        stream.eraseToAnyAsyncSequence()
    }
}

/// Records what the room screen asks the speech synthesizer.
@MainActor
final class RecordingReadAloud: ChatReadAloudServicing {
    private(set) var state: ChatReadAloudState = .idle {
        didSet { onChange?(state) }
    }

    var onChange: ((ChatReadAloudState) -> Void)?
    var onSpeechStart: (() -> Void)?
    private(set) var spoken: [String] = []
    private(set) var stops: [Chat.Id] = []

    func speak(_ text: String, messageId: Chat.MessageId, chatId: Chat.Id) {
        spoken.append(text)
        state = .speaking(chatId: chatId, messageId: messageId)
    }

    func stop(in chatId: Chat.Id) {
        stops.append(chatId)
        state = .idle
    }
}

/// The services of the additions, each one a test double.
@MainActor
struct ChatRoomAdditionsDoubles {
    let personas = PushedPersonaStore()
    let events = RecordingRoomEventPublisher()
    let readAloud = RecordingReadAloud()

    func dependencies() -> ChatRoomAdditionsDependencies {
        ChatRoomAdditionsDependencies(personaStore: personas, eventPublisher: events, readAloud: readAloud)
    }
}
