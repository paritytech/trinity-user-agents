#if TESTNET_FEATURE
    import Foundation
    import TrUAPIHost

    extension MockChatShowcase {
        /// The custom bodies that the mock product draws through the renderer.
        enum CustomType {
            static let card = "showcase.card"
            static let counter = "showcase.counter"
        }

        /// The texts of the message with alternatives, one for each branch.
        static let branchTexts = [
            "Branch 1 of 3: the first alternative of this answer. Use the arrows under it.",
            "Branch 2 of 3: the product swapped the body with an `Edited` update.",
            "Branch 3 of 3: the last alternative. The position moves with `set_message_meta`."
        ]
        static let branchMessageId = "m-branch"
        static let reasoningMessageId = "m-reasoning"

        /// Older messages, so the room screen asks for a second page when the user scrolls up.
        static let historyCount = 60

        /// The messages of `showcase`, oldest first, with their timestamps before `now`.
        static func showcaseMessages(media: MockChatMedia, now: UInt64) -> [ChatMessage] {
            let history = (1 ... historyCount).map { number in
                message(
                    "m-history-\(number)",
                    from: number.isMultiple(of: 3) ? Participant.user : Participant.bob,
                    .text(text: "Earlier message \(number) of \(historyCount), for paging.")
                )
            }
            let showcase = bodies(media: media) + authors() + statuses() + extras()
            return timed(history, endingAt: now - day * 2, spacing: minute * 5)
                + timed(showcase, endingAt: now - minute, spacing: minute * 3)
        }

        /// The meta that the mock product sets on `showcase` after its first page.
        static func showcaseMeta(now: UInt64) -> [(messageId: String, meta: ChatMessageMeta)] {
            [
                (threadRootId, ChatMessageMeta(
                    thread: ChatThreadSummary(
                        roomId: Room.thread,
                        replyCount: UInt32(threadMessages(now: now).count),
                        lastReplyAt: threadMessages(now: now).last?.timestamp,
                        participantIds: [Participant.alice, Participant.bob, Participant.assistant]
                    ),
                    branch: nil,
                    reasoning: nil
                )),
                (branchMessageId, ChatMessageMeta(
                    thread: nil,
                    branch: ChatBranchPosition(index: 1, count: UInt16(branchTexts.count)),
                    reasoning: nil
                )),
                (
                    reasoningMessageId,
                    ChatMessageMeta(thread: nil, branch: nil, reasoning: ChatReasoning(durationMs: 4_200))
                )
            ]
        }

        /// The replies of the thread room, which the host stores. A product writes no message of the user there.
        static func threadMessages(now: UInt64) -> [ChatMessage] {
            let replies = [
                message("t-1", from: Participant.alice, .text(text: "A reply in the thread, stored by the host.")),
                message(
                    "t-2",
                    from: Participant.bob,
                    .richText(text: "Threads are rooms: **every** feature works.", media: [])
                ),
                message(
                    "t-3",
                    from: Participant.assistant,
                    .text(text: "Reply here, and the summary under the root updates.")
                )
            ]
            return timed(replies, endingAt: now - minute * 4, spacing: minute)
        }

        /// The messages of `store`, which the host stores: the bodies that a product writes with `update_feed`.
        static func storeMessages(media: MockChatMedia, now: UInt64) -> [ChatMessage] {
            let messages = [
                message("s-notice", from: Participant.product, .notice(text: "This room is stored by the host.")),
                message("s-text", from: Participant.product, .text(text: "A Text body, stored in Core Data.")),
                message("s-markdown", from: Participant.assistant, .richText(
                    text: "A **RichText** body from a persona, with `code` and a [link](https://polkadot.com).",
                    media: []
                )),
                message("s-image", from: Participant.product, .richText(
                    text: "An inline image, stored as a product body.",
                    media: [image(media.pictures[3], alt: "A green gradient")]
                )),
                message("s-actions", from: Participant.product, .actions(ChatActions(
                    text: "Buttons of a stored message:",
                    actions: [
                        ChatAction(actionId: "store-a", title: "First"),
                        ChatAction(actionId: "store-b", title: "Second")
                    ],
                    layout: .grid
                ))),
                message("s-custom", from: Participant.product, .custom(
                    messageType: CustomType.counter,
                    payload: Data(),
                    previewText: "A counter drawn by the product"
                ))
            ]
            return timed(messages, endingAt: now - minute * 10, spacing: minute)
        }
    }

    // MARK: - Showcase parts

    private extension MockChatShowcase {
        static let minute: UInt64 = 60_000
        static let day: UInt64 = minute * 60 * 24

        static func bodies(media: MockChatMedia) -> [ChatMessage] {
            var messages = [
                message("m-notice", from: Participant.product, .notice(
                    text: "A Notice body: a system row in the middle of the room."
                )),
                message("m-text", from: Participant.product, .text(
                    text: "A Text body is plain. A link such as https://polkadot.com opens after a confirmation."
                )),
                message("m-markdown", from: Participant.product, .richText(text: """
                A **RichText** body without media is Markdown: *italic*, `code`, ~~struck~~ and [a link](https://parity.io).
                - a list item
                - another item
                """, media: [])),
                message("m-image", from: Participant.product, .richText(
                    text: "RichText with one image under the text.",
                    media: [image(media.pictures[0], alt: "A pink gradient")]
                )),
                message("m-gallery", from: Participant.product, .richText(
                    text: nil,
                    media: media.pictures.prefix(3).enumerated().map { image($1, alt: "Gallery image \($0 + 1)") }
                )),
                message("m-file", from: Participant.product, .richText(text: nil, media: [
                    .file(
                        resource: .inline(mimeType: "application/pdf", bytes: media.document),
                        meta: meta("application/pdf", bytes: media.document, alt: "The showcase PDF"),
                        fileName: "Chat Showcase.pdf"
                    )
                ])),
                message("m-actions-column", from: Participant.product, .actions(ChatActions(
                    text: "An Actions body in a column. Pick one:",
                    actions: [
                        ChatAction(actionId: "pick-red", title: "Red"),
                        ChatAction(actionId: "pick-green", title: "Green"),
                        ChatAction(actionId: "pick-blue", title: "Blue")
                    ],
                    layout: .column
                ))),
                message("m-actions-grid", from: Participant.product, .actions(ChatActions(
                    text: nil,
                    actions: ["1", "2", "3", "4"].map { ChatAction(actionId: "grid-\($0)", title: "Option \($0)") },
                    layout: .grid
                ))),
                message("m-card", from: Participant.product, .custom(
                    messageType: CustomType.card,
                    payload: Data("card".utf8),
                    previewText: "A custom body drawn by the product"
                )),
                message("m-counter", from: Participant.product, .custom(
                    messageType: CustomType.counter,
                    payload: Data(),
                    previewText: "A counter drawn by the product"
                ))
            ]

            if let video = media.video {
                let item = ChatMediaItem.video(
                    resource: .inline(mimeType: "video/mp4", bytes: video),
                    meta: ChatMediaMeta(
                        mimeType: "video/mp4",
                        sizeBytes: UInt64(video.count),
                        width: 240,
                        height: 240,
                        placeholder: MockChatMedia.placeholder,
                        alt: "A circle that moves over a colour that changes"
                    )
                )
                let body = ChatMessageBody.richText(text: "RichText with a video, played by the host.", media: [item])
                messages.insert(message("m-video", from: Participant.product, body), at: 5)
            }
            return messages
        }

        static func authors() -> [ChatMessage] {
            [
                message("m-persona", from: Participant.assistant, .text(
                    text: "A persona that the product registered with register_bot, drawn with its name and icon."
                )),
                message("m-persona-support", from: Participant.support, .text(text: "Another persona, Support.")),
                message("m-alice", from: Participant.alice, .text(text: "A Custom participant with an avatar."))
                    .with { $0.reactions = [
                        ChatReactionSummary(emoji: "👍", count: 3, reactedByUser: true),
                        ChatReactionSummary(emoji: "🎉", count: 1, reactedByUser: false)
                    ] },
                message("m-bob", from: Participant.bob, .text(text: "A Custom participant without an avatar. Edited."))
                    .with { $0.editedAt = $0.timestamp },
                message(
                    "m-person",
                    from: Participant.person,
                    .text(text: "A Person of the People chain, person id 1.")
                ),
                message("m-lite", from: Participant.litePerson, .text(text: "A LitePerson, drawn from its account.")),
                message(
                    "m-contact",
                    from: Participant.contact,
                    .text(text: "A Contact handle that this device does not know.")
                ),
                message(
                    "m-product",
                    from: Participant.otherProduct,
                    .text(text: "Another product, drawn from its manifest.")
                ),
                message("m-unknown", from: "nobody", .text(text: "A participant id that the product does not resolve."))
            ]
        }

        static func statuses() -> [ChatMessage] {
            [
                message("m-user-read", from: Participant.user, .text(text: "My message, read."))
                    .with { $0.status = .read },
                message("m-reply", from: Participant.product, .text(text: "A reply to your message, with a quote."))
                    .with { $0.replyTo = "m-user-read" },
                message("m-user-reply", from: Participant.user, .text(text: "My reply to the Markdown message."))
                    .with {
                        $0.replyTo = "m-markdown"
                        $0.status = .delivered
                        $0.reactions = [ChatReactionSummary(emoji: "❤️", count: 2, reactedByUser: false)]
                    },
                message("m-user-sent", from: Participant.user, .text(text: "My message, sent.")),
                message("m-user-preparing", from: Participant.user, .text(text: "My message, still preparing."))
                    .with { $0.status = .preparing },
                message("m-user-failed", from: Participant.user, .text(text: "My message that failed. Retry it."))
                    .with { $0.status = .failed }
            ]
        }

        static func extras() -> [ChatMessage] {
            [
                message(reasoningMessageId, from: Participant.assistant, .text(
                    text: "A reasoning row: the product thought about which parts of a room to show first."
                )),
                message(branchMessageId, from: Participant.assistant, .text(text: branchTexts[0])),
                message(threadRootId, from: Participant.alice, .text(
                    text: "This message has a thread. Tap the summary under it to open the thread room."
                )),
                message("m-end", from: Participant.product, .richText(text: """
                **Try the rest:** the toolbar, the quick replies, the `/` commands, a long press on a message, \
                the participants, search, the call buttons, and typing in the input.
                """, media: []))
            ]
        }
    }

    // MARK: - Builders

    private extension MockChatShowcase {
        static func message(_ messageId: String, from participantId: String, _ body: ChatMessageBody) -> ChatMessage {
            ChatMessage(
                messageId: messageId,
                origin: ChatMessageOrigin(participantId: participantId),
                timestamp: 0,
                body: body,
                status: .sent,
                replyTo: nil,
                editedAt: nil,
                reactions: []
            )
        }

        /// Gives the messages times `spacing` apart, the last one at `end`.
        static func timed(_ messages: [ChatMessage], endingAt end: UInt64, spacing: UInt64) -> [ChatMessage] {
            messages.enumerated().map { index, message in
                message.with { message in
                    let time = end - UInt64(messages.count - 1 - index) * spacing
                    message.editedAt = message.editedAt.map { _ in time + minute }
                    message.timestamp = time
                }
            }
        }

        static func image(_ bytes: Data, alt: String) -> ChatMediaItem {
            .image(resource: .inline(mimeType: "image/jpeg", bytes: bytes), meta: ChatMediaMeta(
                mimeType: "image/jpeg",
                sizeBytes: UInt64(bytes.count),
                width: UInt32(MockChatMedia.pictureSize.width),
                height: UInt32(MockChatMedia.pictureSize.height),
                placeholder: MockChatMedia.placeholder,
                alt: alt
            ))
        }

        static func meta(_ mimeType: String, bytes: Data, alt: String) -> ChatMediaMeta {
            ChatMediaMeta(
                mimeType: mimeType,
                sizeBytes: UInt64(bytes.count),
                width: nil,
                height: nil,
                placeholder: nil,
                alt: alt
            )
        }
    }

    extension MockChatShowcase.Participant {
        static let user = Chat.ProductParticipant.user
        static let product = Chat.ProductParticipant.product
    }

    extension ChatMessage {
        func with(_ change: (inout ChatMessage) -> Void) -> ChatMessage {
            var copy = self
            change(&copy)
            return copy
        }
    }
#endif
