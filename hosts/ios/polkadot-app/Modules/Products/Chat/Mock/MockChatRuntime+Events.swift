#if TESTNET_FEATURE
    import Foundation
    import TrUAPIHost

    /// What the mock product does when the user acts: the events of `action_subscribe`, the commands, the room events
    /// of the additions, and the answers that it streams.
    extension MockChatRuntime {
        /// A declared slash command of `showcase`. Gives false for any other, so the host posts it as text.
        func dispatchCommand(_ command: Chat.Command, roomId: String?) async -> Bool {
            guard roomId == Room.showcase, let declared = MockChatShowcase.Command(rawValue: command.name) else {
                return false
            }

            let argument = command.arguments.trimmingCharacters(in: .whitespacesAndNewlines)
            run { await self.execute(declared, argument: argument) }
            return true
        }

        /// An event of the room screen: a toolbar button, a quick reply, a menu entry or the text of the input.
        func publish(_ event: ChatEvent, roomId: String) async throws {
            switch event {
            case let .messageActionTriggered(messageId, actionId):
                run { await self.pickMessageAction(actionId, on: messageId, in: roomId) }
            case let .toolbarActionTriggered(actionId),
                 let .suggestionSelected(actionId):
                guard roomId == Room.showcase else { return }
                run { await self.press(actionId) }
            case let .participantActionTriggered(participantId, actionId):
                guard roomId == Room.showcase else { return }
                run { await self.notice("You picked “\(actionId)” for the participant \(participantId).") }
            case let .inputChanged(text):
                guard roomId == Room.showcase else { return }
                run { await self.follow(input: text) }
            case .messagePosted,
                 .actionTriggered,
                 .command,
                 .userEdited,
                 .userDeleted,
                 .userReacted,
                 .seen,
                 .roomOpened,
                 .roomClosed:
                break
            }
        }

        /// A button of an `Actions` body.
        func dispatchActionTrigger(roomId: String?, messageId: String, actionId: String) async {
            guard let roomId else { return }

            let productMessageId = Chat.ProductMessageId.product(messageId, in: host.chatId(roomId))
            let text = "You pressed “\(actionId)”."
            if roomId == Room.showcase {
                run { await self.say(text, replyTo: productMessageId) }
            } else {
                run { await self.sayStored(text, in: roomId) }
            }
        }

        /// A room event: a call button, an answer to a ringing call, the call bar, a branch arrow or a mute.
        func receive(_ event: Chat.RoomEvent, roomId: String) async {
            guard roomId == Room.showcase else { return }

            switch event {
            case let .callRequested(video):
                await startCall(video: video)
            case let .callAnswered(_, accept):
                if accept {
                    await startCall(video: false)
                } else {
                    await endCall(reason: "You declined the call.")
                }
            case .callOpened:
                await notice("A product without a media session opens its own call page here.")
            case .callEnded:
                await endCall(reason: "You ended the call.")
            case let .branchSelected(messageId, index):
                await selectBranch(index, of: messageId)
            case let .userStateChanged(state):
                let mutedUntil = state.mutedUntil.map(String.init) ?? "none"
                logger.debug("Chat showcase: the user muted it until \(mutedUntil), archived \(state.archived)")
            case .woken,
                 .callSignal:
                break
            }
        }

        /// The product answers a message of the user: it marks the message delivered, types, answers, and marks
        /// the message read.
        func answer(_ userMessage: ChatMessage) async {
            if userMessage.body.text == MockChatShowcase.suggestions.first?.text {
                await stream()
                return
            }

            await pause(400)
            await deliver(.statusChanged(messageId: userMessage.messageId, status: .delivered))
            await setActivity("Assistant is typing…")
            await pause(1_200)
            let reply =
                switch userMessage.body {
                case let .richText(_, media) where !media.isEmpty: "Thanks for the \(media.count) attachment(s)."
                default: "You said: “\(userMessage.body.text ?? "")”. Try “reject” or “fail” too."
                }
            await say(reply, as: Participant.assistant, replyTo: userMessage.messageId)
            await setActivity(nil)
            await deliver(.statusChanged(messageId: userMessage.messageId, status: .read))
        }

        /// A message of the user in a room that the host stores. In the thread, the summary under the root counts it.
        func answerStored(_ text: String, in roomId: String) async {
            await pause(600)
            await sayStored("Stored: “\(text)”.", as: Participant.assistant, in: roomId)

            guard roomId == Room.thread else { return }
            threadReplies += 2
            let meta = ChatMessageMeta(
                thread: ChatThreadSummary(
                    roomId: Room.thread,
                    replyCount: UInt32(threadReplies),
                    lastReplyAt: Self.now(),
                    participantIds: [Participant.assistant, Participant.alice, Participant.bob]
                ),
                branch: nil,
                reasoning: nil
            )
            await perform("update the thread summary") {
                try await host.setMeta(Room.showcase, messageId: MockChatShowcase.threadRootId, meta)
            }
        }
    }

    // MARK: - Scripts

    extension MockChatRuntime {
        func execute(_ command: MockChatShowcase.Command, argument: String) async {
            switch command {
            case .stream: await stream()
            case .typing: await typeForAWhile()
            case .ring: await ring(video: argument == "video")
            case .call: await startCall(video: argument == "video")
            case .hangup: await endCall(reason: "You hung up with /hangup.")
            case .footer: await setFooter(MockChatShowcase.Footer(rawValue: argument) ?? footer.next)
            case .preview: await setPreview(MockChatShowcase.Preview(rawValue: argument) ?? .default)
            case .notice: await notice(argument.isEmpty ? "A notice that /notice posted." : argument)
            case .burst: await burst()
            }
        }

        func press(_ actionId: String) async {
            switch actionId {
            case "ring": await ring(video: false)
            case "stream": await stream()
            case "footer": await setFooter(footer.next)
            default: await notice("The showcase got “\(actionId)”.")
            }
        }

        func pickMessageAction(_ actionId: String, on messageId: String, in roomId: String) async {
            guard roomId == Room.showcase else {
                await sayStored("You picked “\(actionId)” on a stored message.", in: roomId)
                return
            }

            switch actionId {
            case MockChatMenus.Action.hide:
                await deliver(.deleted(messageId: messageId))
            case MockChatMenus.Action.react:
                await deliver(.reactionPlaced(messageId: messageId, participantId: Participant.alice, emoji: "🎉"))
            case MockChatMenus.Action.refresh:
                renders.reset(messageId)
                await notice("The product drew the body again.")
            default:
                await notice("You picked “\(actionId)” on a message.")
            }
        }

        /// A reasoning row that streams and collapses, then an answer that streams (07-LATER §9.4).
        func stream() async {
            let started = Self.now()
            await setActivity("Assistant is thinking…")
            let reasoning = nextMessageId("reasoning")
            await deliver(.new(message(reasoning, from: Participant.assistant, .text(text: ""), status: .preparing)))
            await perform("mark the reasoning row") {
                try await host.setMeta(Room.showcase, messageId: reasoning, ChatMessageMeta(
                    thread: nil,
                    branch: nil,
                    reasoning: ChatReasoning(durationMs: nil)
                ))
            }
            await streamText(
                "The user asked for a stream. I edit this row while I think, then I mark it sent.",
                into: reasoning
            )
            await deliver(.statusChanged(messageId: reasoning, status: .sent))
            await perform("time the reasoning row") {
                try await host.setMeta(Room.showcase, messageId: reasoning, ChatMessageMeta(
                    thread: nil,
                    branch: nil,
                    reasoning: ChatReasoning(durationMs: UInt32(clamping: Self.now() - started))
                ))
            }

            await setActivity("Assistant is typing…")
            let answer = nextMessageId("answer")
            await deliver(.new(message(answer, from: Participant.assistant, .text(text: ""), status: .preparing)))
            await streamText(
                "A product streams a long answer as `New` with the status Preparing, then `Edited` as "
                    + "text arrives, then `StatusChanged` to Sent.",
                into: answer
            )
            await deliver(.statusChanged(messageId: answer, status: .sent))
            await setActivity(nil)
        }

        func streamText(_ text: String, into messageId: String) async {
            var shown = ""
            for word in text.split(separator: " ") {
                shown += shown.isEmpty ? String(word) : " \(word)"
                await deliver(.edited(messageId: messageId, body: .text(text: shown), editedAt: Self.now()))
                await pause(90)
            }
        }

        func typeForAWhile() async {
            await setActivity("Alice is typing…", about: Participant.alice)
            await pause(5_000)
            await setActivity(nil)
        }

        /// Twenty messages at once: the room draws at most one list update per frame.
        func burst() async {
            for number in 1 ... 20 {
                await say(
                    "Burst message \(number) of 20",
                    as: number.isMultiple(of: 2) ? Participant.alice : Participant.bob
                )
            }
        }

        func ring(video: Bool) async {
            await pause(1_000)
            let call = ChatRoomCall.ringing(callId: nextMessageId("call"), video: video, caller: Participant.alice)
            await perform("ring") { try await host.setCall(Room.showcase, call) }
        }

        func startCall(video: Bool) async {
            let call = ChatRoomCall.active(
                callId: nextMessageId("call"),
                video: video,
                startedAt: Self.now(),
                mediaSession: nil
            )
            await perform("start a call") { try await host.setCall(Room.showcase, call) }
            await notice(video ? "A video call is active. Tap the bar or End." :
                "A call is active. Tap the bar or End.")
        }

        func endCall(reason: String) async {
            await perform("end the call") { try await host.setCall(Room.showcase, nil) }
            await notice(reason)
        }

        func setFooter(_ footer: MockChatShowcase.Footer) async {
            self.footer = footer
            await perform("set the footer") { try await host.setFooter(Room.showcase, footer.wireValue) }
        }

        func setPreview(_ preview: MockChatShowcase.Preview) async {
            await perform("set the preview") { try await host.setPreview(
                Room.showcase,
                preview.wireValue(now: Self.now())
            ) }
            await notice("The chat list shows the preview “\(preview.rawValue)”.")
        }

        /// The quick replies follow the text of the input, for a room that declares `input_events`.
        func follow(input text: String) async {
            let suggestions = text.isEmpty ? MockChatShowcase.suggestions : MockChatShowcase
                .suggestions(whileTyping: text)
            await perform("set the quick replies") { try await host.setSuggestions(Room.showcase, suggestions) }
        }

        /// The product shows another alternative of a message, then moves its position.
        func selectBranch(_ index: UInt16, of messageId: String) async {
            let texts = MockChatShowcase.branchTexts
            guard messageId == MockChatShowcase.branchMessageId,
                  (1 ... texts.count).contains(Int(index)) else { return }

            await deliver(.edited(messageId: messageId, body: .text(text: texts[Int(index) - 1]), editedAt: Self.now()))
            let meta = ChatMessageMeta(
                thread: nil,
                branch: ChatBranchPosition(index: index, count: UInt16(texts.count)),
                reasoning: nil
            )
            await perform("move the branch") { try await host.setMeta(Room.showcase, messageId: messageId, meta) }
        }
    }
#endif
