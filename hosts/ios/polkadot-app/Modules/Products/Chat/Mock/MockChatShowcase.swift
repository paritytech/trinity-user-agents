#if TESTNET_FEATURE
    import Foundation
    import TrUAPIHost
    import UIKit

    /// What the mock product declares: three rooms, their participants and personas, and every declaration that a
    /// product can set, in the wire values of Chat Extension v2.
    ///
    /// - `showcase` keeps its messages in the product (`product_source`) and declares every part of a room.
    /// - `showcase-thread` is the thread of one message of `showcase`, stored by the host.
    /// - `store` is stored by the host and shows the same bodies through the host store.
    enum MockChatShowcase {
        enum Room {
            static let showcase = "showcase"
            static let thread = "showcase-thread"
            static let store = "store"
        }

        enum Participant {
            static let assistant = "bot:assistant"
            static let support = "bot:support"
            static let alice = "alice"
            static let bob = "bob"
            static let person = "person-1"
            static let litePerson = "lite-1"
            static let contact = "contact-1"
            static let otherProduct = "product-1"

            static func guest(_ number: Int) -> String {
                String(format: "guest-%02d", number)
            }
        }

        enum Persona {
            static let assistantId = "assistant"
            static let supportId = "support"
        }

        static let reactions = ["👍", "❤️", "😂", "🎉", "🤔", "👀"]
        static let guestCount = 30

        /// The message of `showcase` whose thread is `showcase-thread`.
        static let threadRootId = "m-thread-root"
    }

    // MARK: - Rooms

    extension MockChatShowcase {
        static var showcaseMetadata: ChatRoomMetadata {
            ChatRoomMetadata(
                name: "Chat Showcase",
                avatar: .inline(
                    mimeType: "image/png",
                    bytes: MockChatMedia.avatar("sparkles.rectangle.stack", color: .systemIndigo)
                ),
                description: "Every part of a product room, drawn by the host from the answers of a mock product."
            )
        }

        static let showcaseConfig = ChatRoomConfig(
            productSource: true,
            reactionDetails: true,
            participants: true,
            messageActions: true,
            participantActions: true,
            allowedReactions: reactions,
            placeholder: "No messages yet. Say hi!",
            reportSeen: true
        )

        static let showcaseFeatures = ChatRoomFeatures(
            preimageAttachments: false,
            search: true,
            calls: ChatRoomCalls(audio: true, video: true),
            threadOf: nil,
            readAloud: true,
            sealedAttachments: false,
            inputEvents: true
        )

        static var threadMetadata: ChatRoomMetadata {
            ChatRoomMetadata(
                name: "Thread",
                avatar: nil,
                description: "The thread of a message of Chat Showcase, stored by the host."
            )
        }

        static let threadConfig = ChatRoomConfig(
            productSource: false,
            reactionDetails: false,
            participants: false,
            messageActions: false,
            participantActions: false,
            allowedReactions: nil,
            placeholder: "No replies yet",
            reportSeen: false
        )

        static let threadFeatures = ChatRoomFeatures(
            preimageAttachments: false,
            search: false,
            calls: nil,
            threadOf: ChatThreadParent(roomId: Room.showcase, messageId: threadRootId),
            readAloud: false,
            sealedAttachments: false,
            inputEvents: false
        )

        static var storeMetadata: ChatRoomMetadata {
            ChatRoomMetadata(
                name: "Showcase (host store)",
                avatar: .inline(mimeType: "image/png", bytes: MockChatMedia.avatar("tray.full", color: .systemTeal)),
                description: "A room whose messages the host stores, with the same bodies as Chat Showcase."
            )
        }

        static let storeConfig = ChatRoomConfig(
            productSource: false,
            reactionDetails: false,
            participants: false,
            messageActions: true,
            participantActions: false,
            allowedReactions: nil,
            placeholder: "The host stores this room",
            reportSeen: true
        )
    }

    // MARK: - Declarations

    extension MockChatShowcase {
        static var toolbar: [ChatToolbarAction] {
            [
                toolbarAction("ring", "Ring me", symbol: "phone.arrow.down.left", handler: .callback),
                toolbarAction("stream", "Stream an answer", symbol: "sparkles", handler: .callback),
                toolbarAction(
                    "footer",
                    "Next footer",
                    symbol: "rectangle.bottomthird.inset.filled",
                    handler: .callback
                ),
                toolbarAction(
                    "docs",
                    "Open polkadot.com",
                    symbol: "safari",
                    handler: .deeplink(url: "https://polkadot.com")
                )
            ]
        }

        static let suggestions = [
            ChatSuggestion(suggestionId: "stream", text: "Stream an answer", sendsText: true),
            ChatSuggestion(suggestionId: "ring", text: "Ring me", sendsText: false),
            ChatSuggestion(suggestionId: "footer", text: "Next footer", sendsText: false),
            ChatSuggestion(suggestionId: "thanks", text: "Thanks! 🙏", sendsText: true)
        ]

        /// The quick replies while the user types `text`, for a room that declares `input_events`.
        static func suggestions(whileTyping text: String) -> [ChatSuggestion] {
            [
                ChatSuggestion(suggestionId: "shout", text: text.uppercased(), sendsText: true),
                ChatSuggestion(suggestionId: "polite", text: "\(text), please", sendsText: true)
            ]
        }

        /// The slash commands of `showcase`. Each one shows a part of a room that the messages alone do not.
        enum Command: String, CaseIterable {
            case stream
            case typing
            case ring
            case call
            case hangup
            case footer
            case preview
            case notice
            case burst

            var spec: ChatCommandSpec {
                ChatCommandSpec(name: rawValue, description: description, argumentHint: argumentHint)
            }

            private var description: String {
                switch self {
                case .stream: "Stream a reasoning row and an answer"
                case .typing: "Show the activity line for five seconds"
                case .ring: "Ring the room as an incoming call"
                case .call: "Show the bar of an active call"
                case .hangup: "Clear the call of the room"
                case .footer: "Replace the footer of the room"
                case .preview: "Replace the chat list preview"
                case .notice: "Post a notice row"
                case .burst: "Post 20 messages at once"
                }
            }

            private var argumentHint: String? {
                switch self {
                case .ring,
                     .call: "<audio|video>"
                case .footer: "<" + Footer.allCases.map(\.rawValue).joined(separator: "|") + ">"
                case .preview: "<" + Preview.allCases.map(\.rawValue).joined(separator: "|") + ">"
                case .notice: "<text>"
                case .stream,
                     .typing,
                     .hangup,
                     .burst: nil
                }
            }
        }

        static let commands = Command.allCases.map(\.spec)

        /// The footers that the toolbar button and `/footer` go through, in order.
        enum Footer: String, CaseIterable {
            case input
            case payment
            case empty
            case custom

            var wireValue: ChatRoomFooter {
                switch self {
                case .input:
                    .input(ChatInputConfig(
                        placeholder: "Message the showcase…",
                        photo: true,
                        video: true,
                        file: true,
                        payment: nil
                    ))
                case .payment:
                    .input(ChatInputConfig(
                        placeholder: "Pay or message the showcase…",
                        photo: true,
                        video: false,
                        file: false,
                        payment: .enabled(destination: Data(repeating: 0x42, count: 32))
                    ))
                case .empty:
                    .empty
                case .custom:
                    .custom
                }
            }

            var next: Footer {
                let all = Self.allCases
                let index = all.firstIndex(of: self) ?? all.startIndex
                return all[(index + 1) % all.count]
            }
        }

        /// The chat list previews that `/preview` sets.
        enum Preview: String, CaseIterable {
            case `default`
            case pinned
            case badge
            case image
            case ongoing
            case hidden

            func wireValue(now: UInt64) -> ChatRoomPreview {
                switch self {
                case .default:
                    ChatRoomPreview(order: .timestamp, timestamp: nil, badge: nil, text: nil)
                case .pinned:
                    ChatRoomPreview(
                        order: .pinToTop,
                        timestamp: now,
                        badge: .unreadCount(count: 2),
                        text: ChatRoomPreviewText(text: "Pinned: everything a product can draw", ongoing: false)
                    )
                case .badge:
                    ChatRoomPreview(order: .timestamp, timestamp: now, badge: .unreadCount(count: 42), text: nil)
                case .image:
                    ChatRoomPreview(
                        order: .timestamp,
                        timestamp: now,
                        badge: .image(resource: .inline(mimeType: "image/png", bytes: MockChatMedia.icon("star.fill"))),
                        text: ChatRoomPreviewText(text: "A badge drawn from an image", ongoing: false)
                    )
                case .ongoing:
                    ChatRoomPreview(
                        order: .timestamp,
                        timestamp: now,
                        badge: .hidden,
                        text: ChatRoomPreviewText(text: "Assistant is typing", ongoing: true)
                    )
                case .hidden:
                    ChatRoomPreview(order: .timestamp, timestamp: now, badge: .hidden, text: nil)
                }
            }
        }

        private static func toolbarAction(
            _ actionId: String,
            _ label: String,
            symbol: String,
            handler: ChatActionHandler
        ) -> ChatToolbarAction {
            ChatToolbarAction(
                actionId: actionId,
                label: label,
                icon: .inline(mimeType: "image/png", bytes: MockChatMedia.icon(symbol)),
                handler: handler
            )
        }
    }

    // MARK: - Participants

    extension MockChatShowcase {
        struct PersonaSpec {
            let botId: String
            let name: String
            let icon: String
        }

        static var personas: [PersonaSpec] {
            [
                PersonaSpec(
                    botId: Persona.assistantId,
                    name: "Assistant",
                    icon: MockChatMedia.dataURL(MockChatMedia.avatar("sparkles", color: .systemPurple))
                ),
                PersonaSpec(
                    botId: Persona.supportId,
                    name: "Support",
                    icon: MockChatMedia.dataURL(MockChatMedia.avatar("lifepreserver", color: .systemGreen))
                )
            ]
        }

        /// One participant of each kind, then guests, so the participant list has more than one page.
        static var participants: [ChatParticipantEntry] {
            let named: [ChatParticipantEntry] = [
                ChatParticipantEntry(participantId: Participant.assistant, participant: .custom(
                    nickname: "Assistant",
                    avatar: .inline(
                        mimeType: "image/png",
                        bytes: MockChatMedia.avatar("sparkles", color: .systemPurple)
                    )
                )),
                ChatParticipantEntry(participantId: Participant.support, participant: .custom(
                    nickname: "Support",
                    avatar: nil
                )),
                ChatParticipantEntry(participantId: Participant.alice, participant: .custom(
                    nickname: "Alice",
                    avatar: .inline(
                        mimeType: "image/png",
                        bytes: MockChatMedia.avatar("heart.fill", color: .systemPink)
                    )
                )),
                ChatParticipantEntry(
                    participantId: Participant.bob,
                    participant: .custom(nickname: "Bob", avatar: nil)
                ),
                ChatParticipantEntry(participantId: Participant.person, participant: .person(personId: 1)),
                ChatParticipantEntry(
                    participantId: Participant.litePerson,
                    participant: .litePerson(account: Data(repeating: 0x11, count: 32))
                ),
                ChatParticipantEntry(
                    participantId: Participant.contact,
                    participant: .contact(handle: ContactHandle(bytes: Data(repeating: 0x22, count: 32)))
                ),
                ChatParticipantEntry(
                    participantId: Participant.otherProduct,
                    participant: .product(productId: "truapi-playground.dot")
                )
            ]
            let guests = (1 ... guestCount).map { number in
                ChatParticipantEntry(
                    participantId: Participant.guest(number),
                    participant: .custom(nickname: "Guest \(number)", avatar: nil)
                )
            }
            return named + guests
        }
    }
#endif
