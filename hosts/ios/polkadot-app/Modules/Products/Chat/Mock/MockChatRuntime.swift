#if TESTNET_FEATURE
    import Foundation
    import Products
    import TrUAPIHost
    import UIKitExt

    /// The runtime of ``MockChatProduct``: a product that keeps its rooms in memory and answers each query of the
    /// room screen at once.
    ///
    /// It takes the place of ``TrUAPIChatHandler`` behind a ``ProductBot``, so everything from the bot inwards is
    /// the code that a published product runs through. It holds no worker, and nothing reaches the core.
    actor MockChatRuntime: ChatRuntimeProtocol, ProductChatQuerying, ChatRoomLifecycle {
        typealias Room = MockChatShowcase.Room
        typealias Participant = MockChatShowcase.Participant

        let productId: ProductId
        let host: MockChatHost
        let logger: LoggerProtocol
        private let surface: ProductChatSurface

        /// The messages of `showcase`, the room that the product keeps.
        var showcase = MockChatConversation(messages: [])
        var footer = MockChatShowcase.Footer.input
        var threadReplies = 0
        /// The state of each tree that the product draws, by the id of its message, and the streams that show it.
        var renders = MockChatRenders()
        private var seeding: Task<Void, Never>?
        private var scripts: [Task<Void, Never>] = []
        private var nextNumber = 0
        private var started = false
        private var disposed = false

        init(productId: ProductId, logger: LoggerProtocol = Logger.shared) {
            let surface = ProductChatSurface()
            self.productId = productId
            self.surface = surface
            self.logger = logger
            host = MockChatHost(productId: productId, messaging: surface)
        }

        /// Binds the chat surface, then creates the rooms and their content. Each query waits for the rooms.
        func start(messagingSupport: ProductsNativeApi.MessagingSupport) async throws {
            guard !started, !disposed else { throw CancellationError() }
            started = true

            surface.bind(messagingSupport, owner: self)
            Self.markStarted(self)
            seeding = Task { await seed() }
            logger.debug("Chat showcase started")
        }

        func dispose() async {
            guard !disposed else { return }
            disposed = true

            seeding?.cancel()
            scripts.forEach { $0.cancel() }
            scripts = []
            renders.finishAll()
            surface.unbind(owner: self)
            Self.markStopped(self)
        }

        @MainActor
        func attach(presentationView _: ControllerBackedProtocol) {}

        // MARK: - ChatRoomLifecycle

        func open(roomId _: String) async throws {
            await seeding?.value
        }

        /// Clears the activity line of the room, as the core does on `RoomClosed`.
        func close(roomId: String) async {
            await perform("clear the activity of \(roomId)") { try await host.setActivity(roomId, nil) }
        }

        // MARK: - ProductChatQuerying

        func messagesPage(_ request: ProductChatMessagesQueryRequest) async throws -> ProductChatMessagesQueryItem {
            await seeding?.value
            guard request.roomId == Room.showcase else { throw ChatQueryError.refused(.unknownRoom) }

            return showcase.page(request.anchor, size: Int(request.pageSize))
        }

        func userAction(_ request: ProductChatUserActionRequest) async throws -> ProductChatUserActionItem {
            await seeding?.value
            guard request.roomId == Room.showcase else { throw ChatQueryError.actionRefused(.unknownRoom) }

            switch request.action {
            case let .newMessage(body, replyTo):
                return try post(body, replyTo: replyTo)
            case let .editMessage(messageId, body):
                try require(showcase.edit(messageId, body: body, at: Self.now()))
            case let .deleteMessage(messageId):
                try require(showcase.remove(messageId))
            case let .reactionPlaced(messageId, emoji):
                try require(showcase.react(messageId, emoji: emoji, placed: true, byUser: true))
            case let .reactionRemoved(messageId, emoji):
                try require(showcase.react(messageId, emoji: emoji, placed: false, byUser: true))
            case .seen:
                break
            case let .retry(messageId):
                try require(showcase.message(messageId) != nil)
                run { await self.deliver(.statusChanged(messageId: messageId, status: .sent)) }
            }
            return .acknowledged
        }

        // MARK: - Messages of the user in the rooms that the host stores

        func onUserMessage(text: String, roomId: String?) async throws {
            guard let roomId else { return }
            run { await self.answerStored(text, in: roomId) }
        }

        func publishUserEvent(_ event: ProductChatUserEvent, roomId: String?) async throws {
            guard let roomId, case let .messagePosted(_, text, _) = event else { return }
            run { await self.answerStored(text, in: roomId) }
        }
    }

    // MARK: - Helpers for the extensions

    extension MockChatRuntime {
        static func now() -> UInt64 {
            Date().toChatTimestamp()
        }

        /// A new message id, unique for the life of the runtime.
        func nextMessageId(_ prefix: String) -> String {
            nextNumber += 1
            return "\(prefix)-\(Self.now())-\(nextNumber)"
        }

        func message(
            _ messageId: String,
            from participantId: String,
            _ body: ChatMessageBody,
            status: ChatMessageStatus = .sent
        ) -> ChatMessage {
            ChatMessage(
                messageId: messageId,
                origin: ChatMessageOrigin(participantId: participantId),
                timestamp: Self.now(),
                body: body,
                status: status,
                replyTo: nil,
                editedAt: nil,
                reactions: []
            )
        }

        /// Runs a script of the product, for example an answer that streams. Dispose cancels it.
        func run(_ script: @escaping @Sendable () async -> Void) {
            guard !disposed else { return }
            scripts.removeAll { $0.isCancelled }
            scripts.append(Task { await script() })
        }

        /// Pushes one update of `showcase` to its open screens, and keeps it.
        func deliver(_ update: ChatFeedUpdate) async {
            apply(update)
            await perform("push an update to the showcase") {
                try await host.push(update, roomId: Room.showcase, persist: false)
            }
        }

        /// Writes a text to a room that the host stores.
        func sayStored(_ text: String, as participantId: String = Participant.product, in roomId: String) async {
            let update = ChatFeedUpdate.new(message(nextMessageId("store"), from: participantId, .text(text: text)))
            await perform("write to \(roomId)") { try await host.push(update, roomId: roomId, persist: true) }
        }

        func notice(_ text: String) async {
            await deliver(.new(message(nextMessageId("notice"), from: Participant.product, .notice(text: text))))
        }

        func say(_ text: String, as participantId: String = Participant.product, replyTo: String? = nil) async {
            await deliver(.new(message(nextMessageId("say"), from: participantId, .text(text: text)).with {
                $0.replyTo = replyTo
            }))
        }

        func setActivity(_ text: String?, about participantId: String? = Participant.assistant) async {
            let activity = text.map { ChatActivity(text: $0, participantId: participantId) }
            await perform("set the activity line") { try await host.setActivity(Room.showcase, activity) }
        }

        /// Logs a failed call to the host instead of throwing it: a showcase that cannot set one part still shows
        /// the others.
        func perform(_ what: String, _ call: () async throws -> Void) async {
            do {
                try await call()
            } catch {
                logger.error("Chat showcase could not \(what): \(error)")
            }
        }

        func pause(_ milliseconds: Int) async {
            try? await Task.sleep(for: .milliseconds(milliseconds))
        }
    }

    // MARK: - Private

    private extension MockChatRuntime {
        func seed() async {
            let media = await MockChatMedia.make()
            let now = Self.now()
            showcase = MockChatConversation(messages: MockChatShowcase.showcaseMessages(media: media, now: now))
            threadReplies = MockChatShowcase.threadMessages(now: now).count

            await perform("register its personas") {
                for persona in MockChatShowcase.personas {
                    try await host.registerPersona(botId: persona.botId, name: persona.name, icon: persona.icon)
                }
            }
            await perform("declare the showcase room") { try await declareShowcase(now: now) }
            await perform("declare the thread room") { try await declareThread(now: now) }
            await perform("declare the stored room") { try await declareStore(media: media, now: now) }
            logger.debug("Chat showcase declared its rooms")
        }

        func declareShowcase(now: UInt64) async throws {
            let room = Room.showcase
            try await host.createRoom(
                room,
                metadata: MockChatShowcase.showcaseMetadata,
                config: MockChatShowcase.showcaseConfig,
                preview: MockChatShowcase.Preview.pinned.wireValue(now: now)
            )
            try await host.setFeatures(room, MockChatShowcase.showcaseFeatures)
            try await host.setToolbar(room, MockChatShowcase.toolbar)
            try await host.setSuggestions(room, MockChatShowcase.suggestions)
            try await host.setCommands(room, MockChatShowcase.commands)
            try await host.setFooter(room, footer.wireValue)
            for (messageId, meta) in MockChatShowcase.showcaseMeta(now: now) {
                try await host.setMeta(room, messageId: messageId, meta)
            }
        }

        func declareThread(now: UInt64) async throws {
            let room = Room.thread
            try await host.createRoom(
                room,
                metadata: MockChatShowcase.threadMetadata,
                config: MockChatShowcase.threadConfig,
                preview: nil
            )
            try await host.setFeatures(room, MockChatShowcase.threadFeatures)
            try await host.setFooter(room, .input(ChatInputConfig(
                placeholder: "Reply in the thread…",
                photo: false,
                video: false,
                file: false,
                payment: nil
            )))
            await storeOnce(MockChatShowcase.threadMessages(now: now), in: room)
        }

        func declareStore(media: MockChatMedia, now: UInt64) async throws {
            try await host.createRoom(
                Room.store,
                metadata: MockChatShowcase.storeMetadata,
                config: MockChatShowcase.storeConfig,
                preview: nil
            )
            await storeOnce(MockChatShowcase.storeMessages(media: media, now: now), in: Room.store)
        }

        /// The host keeps a stored room across launches, so a message that it holds already is refused as a
        /// duplicate. Only the first launch adds them.
        func storeOnce(_ messages: [ChatMessage], in roomId: String) async {
            for message in messages {
                do {
                    try await host.push(.new(message), roomId: roomId, persist: true)
                } catch {
                    logger.debug("Chat showcase keeps \(message.messageId) in \(roomId): \(error)")
                }
            }
        }

        func require(_ found: Bool) throws {
            guard found else { throw ChatQueryError.actionRefused(.unknownMessage) }
        }

        /// The user sends a message to `showcase`. "reject" is refused, "fail" is created as failed, and every other
        /// message gets an answer.
        func post(_ body: ChatMessageBody, replyTo: String?) throws -> ProductChatUserActionItem {
            let text = body.text?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            if text == "reject" {
                throw ChatQueryError
                    .actionRefused(.rejected(reason: "The showcase refuses a message that says “reject”."))
            }

            let created = message(
                nextMessageId("user"),
                from: Participant.user,
                body,
                status: text == "fail" ? .failed : .sent
            )
            .with { $0.replyTo = replyTo }
            showcase.append(created)
            if created.status == .sent {
                run { await self.answer(created) }
            }
            return .messageCreated(created)
        }

        func apply(_ update: ChatFeedUpdate) {
            switch update {
            case let .new(message):
                showcase.append(message)
            case let .edited(messageId, body, editedAt):
                showcase.edit(messageId, body: body, at: editedAt)
            case let .deleted(messageId):
                showcase.remove(messageId)
            case let .reactionPlaced(messageId, participantId, emoji):
                showcase.react(messageId, emoji: emoji, placed: true, byUser: participantId == Participant.user)
            case let .reactionRemoved(messageId, participantId, emoji):
                showcase.react(messageId, emoji: emoji, placed: false, byUser: participantId == Participant.user)
            case let .statusChanged(messageId, status):
                showcase.setStatus(messageId, status)
            }
        }
    }
#endif
