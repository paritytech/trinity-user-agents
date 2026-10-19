#if TESTNET_FEATURE
    import Foundation
    import TrUAPIHost

    /// The bodies that the mock product draws, by the id that the host gives each one, and the streams that show them.
    /// A press changes the state of a body, and each stream that shows it gets the new tree, as a render stream of the
    /// core does.
    struct MockChatRenders {
        enum Body: Equatable {
            case card(typed: String, primaryPresses: Int)
            case counter(Int)
            case footer

            var tree: RendererNode {
                switch self {
                case let .card(typed, primaryPresses): MockChatRenderTrees.card(
                        typed: typed,
                        primaryPresses: primaryPresses
                    )
                case let .counter(count): MockChatRenderTrees.counter(count)
                case .footer: MockChatRenderTrees.footer()
                }
            }

            var initial: Body {
                switch self {
                case .card: .card(typed: "", primaryPresses: 0)
                case .counter: .counter(0)
                case .footer: .footer
                }
            }
        }

        private var bodies: [String: Body] = [:]
        private var streams: [String: [UUID: AsyncThrowingStream<ChatRendererOutput, Error>.Continuation]] = [:]

        /// The body of `key`, which starts as `initial` the first time that the host asks for it.
        mutating func body(_ key: String, initial: Body) -> Body {
            if let body = bodies[key] { return body }
            bodies[key] = initial
            return initial
        }

        mutating func add(
            _ continuation: AsyncThrowingStream<ChatRendererOutput, Error>.Continuation,
            token: UUID,
            for key: String
        ) {
            streams[key, default: [:]][token] = continuation
        }

        mutating func remove(_ token: UUID, for key: String) {
            streams[key]?[token] = nil
        }

        mutating func update(_ key: String, _ change: (Body) -> Body) {
            guard let body = bodies[key] else { return }
            let changed = change(body)
            bodies[key] = changed
            streams[key]?.values.forEach { $0.yield(.native(changed.tree)) }
        }

        /// Draws a body as the first time.
        mutating func reset(_ key: String) {
            update(key) { $0.initial }
        }

        mutating func finishAll() {
            streams.values.flatMap(\.values).forEach { $0.finish() }
            streams = [:]
        }
    }

    extension MockChatRuntime: ProductRoomFooterRendering {
        struct UnknownBody: Error {
            let messageType: String
        }

        func renderMessage(
            roomId _: String?,
            messageId: String,
            messageType: String,
            messageData _: Data
        ) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
            switch messageType {
            case MockChatShowcase.CustomType.card:
                openStream(messageId, initial: .card(typed: "", primaryPresses: 0))
            case MockChatShowcase.CustomType.counter:
                openStream(messageId, initial: .counter(0))
            default:
                AsyncThrowingStream { $0.finish(throwing: UnknownBody(messageType: messageType)) }
            }
        }

        /// A press or a change of text inside a body that the product drew.
        func dispatchEvent(
            roomId _: String?,
            messageId: String,
            messageType _: String?,
            actionId: String,
            payload: String?
        ) async {
            typealias Action = MockChatRenderTrees.Action
            renders.update(messageId) { body in
                switch (body, actionId) {
                case (.counter(let count), Action.increment):
                    .counter(count + 1)
                case (.counter, Action.reset):
                    .counter(0)
                case (.card(let typed, let presses), Action.primary):
                    .card(typed: typed, primaryPresses: presses + 1)
                case (.card(_, let presses), Action.typed):
                    .card(typed: payload ?? "", primaryPresses: presses)
                default:
                    body
                }
            }
        }

        func renderFooter(roomId: String) async -> AsyncThrowingStream<ChatRendererOutput, Error> {
            openStream(footerKey(roomId), initial: .footer)
        }

        /// The quick answers of the custom footer post as the user, and its last button brings the input back.
        func dispatchFooterAction(roomId: String, actionId: String, payload _: String?) async {
            guard roomId == Room.showcase else { return }

            switch actionId {
            case MockChatRenderTrees.Action.footerYes,
                 MockChatRenderTrees.Action.footerNo:
                let text = actionId == MockChatRenderTrees.Action.footerYes ? "👍" : "👎"
                let posted = message(nextMessageId("footer"), from: Participant.user, .text(text: text))
                run {
                    await self.deliver(.new(posted))
                    await self.answer(posted)
                }
            case MockChatRenderTrees.Action.footerInput:
                run { await self.setFooter(.input) }
            default:
                break
            }
        }
    }

    private extension MockChatRuntime {
        func footerKey(_ roomId: String) -> String {
            "footer/\(roomId)"
        }

        func openStream(
            _ key: String,
            initial: MockChatRenders.Body
        ) -> AsyncThrowingStream<ChatRendererOutput, Error> {
            let body = renders.body(key, initial: initial)
            let (stream, continuation) = AsyncThrowingStream<ChatRendererOutput, Error>.makeStream()
            let token = UUID()
            continuation.onTermination = { [weak self] _ in
                Task { await self?.closeStream(token, for: key) }
            }
            continuation.yield(.native(body.tree))
            renders.add(continuation, token: token, for: key)
            return stream
        }

        func closeStream(_ token: UUID, for key: String) {
            renders.remove(token, for: key)
        }
    }
#endif
