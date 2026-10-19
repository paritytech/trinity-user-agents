#if TESTNET_FEATURE
    import Foundation
    import Keystore_iOS
    import os
    import Products

    /// A chat product that lives in the app, so a tester can check how the host draws everything that a product can
    /// declare and send (Chat Extension v2 and its additions) without a worker or a network.
    ///
    /// Its bot is a ``ProductBot`` like any other, so the room screen, the chat list and the store take the same
    /// paths as for a published product. Only the runtime differs: ``MockChatRuntime`` answers from memory.
    /// Debug settings turn it on, and the app must restart to rebuild the bots.
    enum MockChatProduct {
        /// A dotNS name, so that room links of the showcase open it. The bot never resolves it: the provider adds
        /// the bot without the resolve path.
        static let product = Product(id: "chat-showcase.dot", name: "Chat Showcase")

        static let description = "A product in the app that draws every part of a product room."

        /// The bot of the mock product, or nil while debug settings keep it off.
        static func bot(settingsManager: SettingsManagerProtocol) -> ProductBot? {
            guard settingsManager.isMockChatProductEnabled else { return nil }

            return ProductBot(
                product: product,
                description: description,
                runtime: MockChatRuntime(productId: product.id),
                resolveImage: MockChatMedia.widgetImages
            )
        }

        /// Hands the room events of the mock product to its runtime, and every other event to `publisher`.
        static func routingRoomEvents(to publisher: ChatRoomEventPublishing) -> ChatRoomEventPublishing {
            MockChatRoomEventRouter(fallback: publisher)
        }
    }

    /// The room events of a room (calls, branches, mute and archive) go to the core, not to the bot. The core does not
    /// know the mock product, so this router gives them to its runtime instead.
    private struct MockChatRoomEventRouter: ChatRoomEventPublishing {
        let fallback: ChatRoomEventPublishing

        func publish(_ event: Chat.RoomEvent, productId: ChatExtension.Id, roomId: String) throws {
            guard productId == MockChatProduct.product.extensionId else {
                try fallback.publish(event, productId: productId, roomId: roomId)
                return
            }

            guard let runtime = MockChatRuntime.started else {
                throw MockChatRuntime.NotStarted()
            }

            Task { await runtime.receive(event, roomId: roomId) }
        }
    }

    extension MockChatRuntime {
        struct NotStarted: Error {}

        private static let startedRuntime = OSAllocatedUnfairLock<WeakRuntime>(initialState: WeakRuntime())

        private struct WeakRuntime: @unchecked Sendable {
            weak var runtime: MockChatRuntime?
        }

        /// The runtime of the bot that the chat store kept. The bot provider builds a new bot on each emission, and
        /// only the one that starts serves the rooms.
        static var started: MockChatRuntime? {
            startedRuntime.withLock { $0.runtime }
        }

        static func markStarted(_ runtime: MockChatRuntime) {
            startedRuntime.withLock { $0.runtime = runtime }
        }

        static func markStopped(_ runtime: MockChatRuntime) {
            startedRuntime.withLock { state in
                guard state.runtime === runtime else { return }
                state.runtime = nil
            }
        }
    }
#endif
