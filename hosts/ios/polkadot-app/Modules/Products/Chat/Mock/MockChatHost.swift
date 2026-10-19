#if TESTNET_FEATURE
    import Foundation
    import Products
    import TrUAPIHost

    /// What the mock product asks of the host, in the wire values of Chat Extension v2.
    ///
    /// The calls take the path of ``RustChatExecutionBridge``: each wire value goes through its `+Domain` mapper into
    /// ``ProductChatMessaging``, and a feed update of a room that the product keeps goes to the open screens of the
    /// room. So the mock checks the mappers and the store as a product worker does.
    struct MockChatHost: Sendable {
        let productId: ProductId
        let messaging: any ProductChatMessaging
        var feedListeners: ProductFeedListeners = .shared
        var pageCache: ProductMessageCaching = ProductMessageCache.shared
        var preparingMessages: ChatPreparingMessageTracking = ChatPreparingMessageStore.shared

        func chatId(_ roomId: String) -> Chat.Id {
            .chatExtension(productId, roomId: roomId)
        }

        /// `create_room`: the room, then its metadata, configuration and preview.
        func createRoom(
            _ roomId: String,
            metadata: ChatRoomMetadata,
            config: ChatRoomConfig,
            preview: ChatRoomPreview?
        ) async throws {
            _ = try await messaging.createRoom(CreateRoomRequest(roomId: roomId, name: metadata.name, icon: nil))
            try await messaging.setRoomMetadata(
                roomId: roomId,
                name: metadata.name,
                description: metadata.description,
                avatar: metadata.avatarImage
            )
            try await messaging.setRoomConfig(roomId: roomId, config: Chat.RoomConfig(config))
            if let preview {
                try await setPreview(roomId, preview)
            }
        }

        func setPreview(_ roomId: String, _ preview: ChatRoomPreview) async throws {
            try await messaging.setRoomPreview(roomId: roomId, preview: Chat.RoomPreview(preview))
        }

        func setFooter(_ roomId: String, _ footer: ChatRoomFooter) async throws {
            try await messaging.setRoomFooterConfig(roomId: roomId, footer: Chat.RoomFooterConfig(footer))
        }

        func setToolbar(_ roomId: String, _ actions: [ChatToolbarAction]) async throws {
            try await messaging.setRoomToolbar(roomId: roomId, actions: actions.map(Chat.RoomToolbarAction.init))
        }

        func setSuggestions(_ roomId: String, _ suggestions: [ChatSuggestion]) async throws {
            try await messaging.setRoomSuggestions(
                roomId: roomId,
                suggestions: suggestions.map(Chat.RoomSuggestion.init)
            )
        }

        func setCommands(_ roomId: String, _ commands: [ChatCommandSpec]) async throws {
            try await messaging.setRoomCommands(roomId: roomId, commands: commands.map(Chat.RoomCommand.init))
        }

        func setActivity(_ roomId: String, _ activity: ChatActivity?) async throws {
            try await messaging.setRoomActivity(roomId: roomId, activity: activity.map(Chat.RoomActivity.init))
        }

        func setFeatures(_ roomId: String, _ features: ChatRoomFeatures) async throws {
            try await messaging.setRoomFeatures(roomId: roomId, features: Chat.RoomFeatures(features))
        }

        func setMeta(_ roomId: String, messageId: String, _ meta: ChatMessageMeta) async throws {
            try await messaging.setMessageMeta(roomId: roomId, messageId: messageId, meta: Chat.MessageMeta(meta))
        }

        func setCall(_ roomId: String, _ call: ChatRoomCall?) async throws {
            try await messaging.setRoomCall(roomId: roomId, call: call.map(Chat.RoomCall.init))
        }

        func registerPersona(botId: String, name: String, icon: String?) async throws {
            _ = try await messaging.registerPersona(botId: botId, name: name, icon: icon)
        }

        /// `update_feed`. A room that the host stores keeps the update. For a room that the product keeps, the open
        /// screens take it, or the cached page when no screen shows the room.
        func push(_ update: ChatFeedUpdate, roomId: String, persist: Bool) async throws {
            let chatId = chatId(roomId)
            let localUpdate = Chat.ProductFeedUpdate(update)

            guard persist else {
                preparingMessages.record(localUpdate, isPreparing: update.isNewAndPreparing, chatId: chatId)
                if await !feedListeners.deliver(update, roomId: roomId, to: chatId) {
                    await pageCache.apply(update, productId: productId, roomId: roomId)
                }
                return
            }

            try await messaging.applyFeedUpdate(roomId: roomId, update: localUpdate)
            preparingMessages.record(localUpdate, isPreparing: update.isNewAndPreparing, chatId: chatId)
        }
    }

    private extension ChatFeedUpdate {
        var isNewAndPreparing: Bool {
            if case let .new(message) = self { message.status == .preparing } else { false }
        }
    }
#endif
