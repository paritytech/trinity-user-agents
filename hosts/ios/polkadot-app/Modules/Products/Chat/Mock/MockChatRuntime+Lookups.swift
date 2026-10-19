#if TESTNET_FEATURE
    import Foundation
    import TrUAPIHost

    /// The menus that the mock product answers. Together they use both menu kinds, hidden built-in entries, icons,
    /// destructive entries and both handlers.
    enum MockChatMenus {
        enum Action {
            static let thread = "thread"
            static let pin = "pin"
            static let react = "react"
            static let hide = "hide"
            static let refresh = "refresh"
            static let mention = "mention"
            static let mute = "mute"
            static let about = "about"
        }

        static func messageMenu(for message: ChatMessage) -> ChatMessageMenu {
            if message.origin.participantId == MockChatShowcase.Participant.user {
                return .extendBuiltIn(actions: [entry(Action.pin, "Pin", symbol: "pin")], hidden: [])
            }

            switch message.body {
            case .custom:
                return .custom(actions: [
                    entry(Action.refresh, "Draw again", symbol: "arrow.clockwise"),
                    entry(
                        "docs",
                        "Open polkadot.com",
                        symbol: "safari",
                        handler: .deeplink(url: "https://polkadot.com")
                    )
                ])
            case .notice:
                return .custom(actions: [])
            case .text,
                 .richText,
                 .actions:
                return .extendBuiltIn(
                    actions: [
                        entry(Action.thread, "Start a thread", symbol: "bubble.left.and.bubble.right"),
                        entry(Action.react, "Alice reacts 🎉", symbol: "party.popper"),
                        entry(Action.hide, "Hide for everyone", symbol: "trash", destructive: true)
                    ],
                    hidden: [.edit, .delete]
                )
            }
        }

        static func participantMenu(for entry: ChatParticipantEntry) -> ChatParticipantMenu {
            if entry.participantId.hasPrefix("bot:") {
                return .custom(actions: [self.entry(Action.about, "About this persona", symbol: "info.circle")])
            }

            guard case .custom = entry.participant else { return .extendBuiltIn(actions: []) }
            return .extendBuiltIn(actions: [
                self.entry(Action.mention, "Mention", symbol: "at"),
                self.entry(Action.mute, "Mute in this room", symbol: "speaker.slash", destructive: true)
            ])
        }

        private static func entry(
            _ actionId: String,
            _ label: String,
            symbol: String,
            destructive: Bool = false,
            handler: ChatActionHandler = .callback
        ) -> ChatProductAction {
            ChatProductAction(
                actionId: actionId,
                label: label,
                icon: .inline(mimeType: "image/png", bytes: MockChatMedia.icon(symbol)),
                destructive: destructive,
                handler: handler
            )
        }
    }

    /// The lookups of the room screen (02-DESIGN §3.3.7 to §3.3.9) and the queries of the additions.
    extension MockChatRuntime: ProductRoomInteracting, ProductChatSearching, ProductChatListSearching,
        ProductChatLinkTaking {
        func messageMenu(
            _ request: ProductChatMessageActionsQueryRequest
        ) async throws -> ProductChatMessageActionsQueryItem {
            ProductChatMessageActionsQueryItem(menu: MockChatMenus.messageMenu(for: request.message))
        }

        func participants(_ request: ProductChatParticipantsQueryRequest) async throws
            -> ProductChatParticipantsQueryItem {
            guard request.roomId == Room.showcase else { throw ChatQueryError.refused(.notAvailable) }

            let all = MockChatShowcase.participants
            switch request.query {
            case .count:
                return .count(count: UInt32(all.count))
            case let .page(cursor, pageSize):
                let start = cursor
                    .flatMap { cursor in all.firstIndex { $0.participantId == cursor }.map { $0 + 1 } } ?? 0
                let end = min(start + Int(pageSize), all.count)
                let page = start < end ? Array(all[start ..< end]) : []
                return .page(participants: page, next: end < all.count ? page.last?.participantId : nil)
            case let .byIds(participantIds):
                return .resolved(participants: all.filter { participantIds.contains($0.participantId) })
            }
        }

        func participantMenu(
            _ request: ProductChatParticipantActionsQueryRequest
        ) async throws -> ProductChatParticipantActionsQueryItem {
            ProductChatParticipantActionsQueryItem(menu: MockChatMenus.participantMenu(for: request.entry))
        }

        /// Who reacted: the user when the summary says so, then named participants for the rest of the count.
        func reactions(_ request: ProductChatReactionsQueryRequest) async throws -> ProductChatReactionsQueryItem {
            guard let message = showcase.message(request.messageId) else {
                throw ChatQueryError.refused(.notAvailable)
            }

            let others = [Participant.alice, Participant.bob, Participant.assistant]
                + (1 ... MockChatShowcase.guestCount).map(Participant.guest)
            let details = message.reactions.flatMap { summary in
                let user = summary.reactedByUser ? [Participant.user] : []
                let rest = others.prefix(max(Int(summary.count) - user.count, 0))
                return (user + rest).map { ChatReactionDetail(participantId: $0, emoji: summary.emoji) }
            }
            return ProductChatReactionsQueryItem(reactions: details)
        }

        func searchPage(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem {
            guard request.roomId == Room.showcase else { throw ChatQueryError.refused(.notAvailable) }

            return showcase.search(request.text, cursor: request.cursor, size: Int(request.pageSize))
        }

        func searchPageIfRunning(_ request: ProductChatSearchQueryRequest) async throws -> ProductChatSearchQueryItem? {
            try await searchPage(request)
        }

        /// A room link with a payload keeps the room of the link, and the product says what it got.
        func openLink(_ request: ProductChatOpenLinkRequest) async throws -> ProductChatOpenLinkItem {
            if request.roomId == Room.showcase {
                run { await self.notice("The room link carried the payload “\(request.payload)”.") }
            }
            return ProductChatOpenLinkItem(roomId: nil)
        }
    }
#endif
