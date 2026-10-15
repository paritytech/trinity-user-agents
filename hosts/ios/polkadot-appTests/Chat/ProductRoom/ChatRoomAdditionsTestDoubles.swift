import AsyncExtensions
import Foundation
import os
@testable import polkadot_app

/// Meta that a test pushes to the room screen.
final class PushedMetaStore: ChatMessageMetaStoring, @unchecked Sendable {
    private let stream: AsyncStream<Chat.MessageMetaIndex>
    private let continuation: AsyncStream<Chat.MessageMetaIndex>.Continuation

    init() {
        (stream, continuation) = AsyncStream<Chat.MessageMetaIndex>.makeStream()
    }

    func push(_ meta: [String: Chat.MessageMeta]) {
        continuation.yield(Chat.MessageMetaIndex(meta))
    }

    func setMeta(_: Chat.MessageMeta, productMessageId _: String, chatId _: Chat.Id) async throws {}

    func metaIndex(for _: Chat.Id) async throws -> Chat.MessageMetaIndex {
        .empty
    }

    func metaUpdates(for _: Chat.Id) -> AnyAsyncSequence<Chat.MessageMetaIndex> {
        stream.eraseToAnyAsyncSequence()
    }
}

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

/// Gives a fixed root for every thread, and records what the parent room hands over.
final class FixedThreadRoots: ChatThreadRootResolving, @unchecked Sendable {
    let root: ChatThreadRoot
    private let state = OSAllocatedUnfairLock(initialState: [(ChatThreadRoot, Chat.Id)]())

    init(root: ChatThreadRoot) {
        self.root = root
    }

    var handedOver: [(root: ChatThreadRoot, threadChatId: Chat.Id)] {
        state.withLock { $0.map { (root: $0.0, threadChatId: $0.1) } }
    }

    func handOver(_ root: ChatThreadRoot, threadChatId: Chat.Id) {
        state.withLock { $0.append((root, threadChatId)) }
    }

    func root(of _: Chat.ThreadParent, threadChatId _: Chat.Id) async -> ChatThreadRoot {
        root
    }
}

/// The services of the additions, each one a test double.
@MainActor
struct ChatRoomAdditionsDoubles {
    let meta = PushedMetaStore()
    let personas = PushedPersonaStore()
    let preparing = ChatPreparingMessageStore()
    let events = RecordingRoomEventPublisher()
    let readAloud = RecordingReadAloud()
    let threadRoots = FixedThreadRoots(root: ChatThreadRoot(
        parentName: "Lobby",
        author: "Game",
        text: "The root"
    ))

    func dependencies(branchTimeout: Duration = .seconds(5)) -> ChatRoomAdditionsDependencies {
        ChatRoomAdditionsDependencies(
            metaStore: meta,
            personaStore: personas,
            preparingMessages: preparing,
            eventPublisher: events,
            readAloud: readAloud,
            threadRoots: threadRoots,
            branchTimeout: branchTimeout
        )
    }
}
