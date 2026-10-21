import Foundation
import UIKit

/// The hooks of the truapi E2E harness for product rooms (P6.10): console markers that tell what the room screen
/// shows, and the actions that the harness asks for. Both exist only in a simulator build with the markers on.
extension ChatProductRoomPresenter {
    /// Writes `[e2e:ui] <key> <room> <value>` when the value of the key changes.
    func mark(_ key: String, _ value: @autoclosure () -> String) {
        #if targetEnvironment(simulator)
            guard TrUAPIE2EMarkers.isEnabled else { return }
            let line = value()
            guard e2eMarks[key] != line else { return }
            e2eMarks[key] = line
            logger.debug("[e2e:ui] \(key) \(roomId) \(line)")
        #endif
    }

    /// Counts the window updates, and writes the time from the open (or the retry) of the room to its first ready
    /// window, for the performance rows of the harness (P7.1, B2).
    func markPerformance(readiness: ChatRoomReadiness, messages count: Int) {
        #if targetEnvironment(simulator)
            guard TrUAPIE2EMarkers.isEnabled else { return }
            e2eUpdates += 1
            guard readiness == .ready, let start = e2eReadyWaitStart else { return }
            e2eReadyWaitStart = nil
            let elapsed = ContinuousClock.now - start
            let milliseconds = Double(elapsed.components.seconds) * 1_000
                + Double(elapsed.components.attoseconds) / 1e15
            e2eMarks["perf-ready"] = nil
            mark("perf-ready", String(format: "ms=%.0f count=%d", milliseconds, count))
        #endif
    }

    /// The newest messages of the window, for the harness: `id:status:kind`.
    func markWindow() {
        mark("window", {
            let newest = messages.values
                .sorted { ($0.order, $0.timestamp) < ($1.order, $1.timestamp) }
                .suffix(3)
                .map { "\(productMessageId($0.messageId)):\($0.status.e2eName):\($0.content.contentType)" }
            return "count=\(messages.count) tail=" + newest.joined(separator: ",")
        }())
    }

    /// The id of a message that the harness names: `last` for the newest message of the product, or an id of the
    /// product.
    func e2eMessageId(_ name: String) -> Chat.MessageId? {
        if name == "last" || name == "lastUser" {
            let fromUser = name == "lastUser"
            return messages.values
                .filter { $0.status.isOutgoing == fromUser && !$0.isSystem }
                .max { ($0.order, $0.timestamp) < ($1.order, $1.timestamp) }?
                .messageId
        }

        return Chat.ProductMessageId.candidates(name, in: chatId).first { messages[$0] != nil }
    }
}

#if targetEnvironment(simulator)
    /// The link that waits for the confirmation of the user. The harness confirms it with the action `confirm`.
    @MainActor
    enum TrUAPIE2EPendingConfirmation {
        static var onOpen: (() -> Void)?
    }

    /// The public-storage notice that waits for the answer of the user (P10.3). The harness answers it with the
    /// action `notice` (`accept` or `decline`).
    @MainActor
    enum TrUAPIE2EPendingNotice {
        static var onAnswer: ((Bool) -> Void)?

        static func answer(_ argument: String?) {
            let onAnswer = onAnswer
            self.onAnswer = nil
            onAnswer?(argument == "accept")
        }
    }

    extension ChatProductRoomPresenter {
        func observeE2EActions() -> NSObjectProtocol? {
            guard TrUAPIE2EMarkers.isEnabled else { return nil }

            return NotificationCenter.default.addObserver(
                forName: TrUAPIE2EHostAction.notification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let action = notification.object as? TrUAPIE2EHostAction else { return }
                MainActor.assumeIsolated { self?.runE2E(action) }
            }
        }

        // swiftlint:disable:next cyclomatic_complexity
        private func runE2E(_ action: TrUAPIE2EHostAction) {
            // A full-screen media viewer takes the room screen out of the window, so `dismiss` runs before the
            // check of the window.
            if action.action == "dismiss", action.room == roomId {
                dismissMediaViewer()
                return
            }
            guard action.room == roomId, view?.controller.view.window != nil else { return }
            logger.debug("[e2e:ui] action \(roomId) \(action.action) \(action.argument ?? "") \(action.target ?? "")")

            switch action.action {
            case "send": action.argument.map { sendText?($0) }
            case "menu": runMenu(message: action.target ?? "last", select: action.argument)
            case "confirm": confirmPendingLink()
            case "participants": showParticipants()
            case "participant": runParticipant(action.target)
            case "reactions": action.target.flatMap(e2eMessageId).map { showReactionDetails(for: $0) }
            case "type":
                NotificationCenter.default.post(
                    name: TrUAPIE2EHostAction.inputTextNotification,
                    object: action.argument ?? ""
                )
            case "attach": runAttach(path: action.argument, kind: action.target)
            case "notice": TrUAPIE2EPendingNotice.answer(action.argument)
            case "notice-reset":
                staging.notice.reset(productId: appearanceFactory.productId)
                mark("notice", "reset")
            case "b7": measureStagingCrossing(runs: action.argument.flatMap(Int.init) ?? 5)
            case "outbox-clear": clearOutbox()
            case "back": view?.controller.navigationController?.popViewController(animated: false)
            case "button":
                action.target.flatMap(e2eMessageId).map { messageId in
                    interactor?.processAction(.productButton(messageId: messageId, actionId: action.argument ?? ""))
                }
            case "footer-action":
                Task { [footerRenderer, roomId] in
                    await footerRenderer.dispatchFooterAction(
                        roomId: roomId,
                        actionId: action.argument ?? "",
                        payload: nil
                    )
                }
            case "older": interactor?.loadOlderMessages()
            case "jump": action.target.map { interactor?.jump(to: $0) }
            case "retry": action.target.flatMap(e2eMessageId).map { interactor?.retrySend(messageId: $0) }
            case "delete": action.target.flatMap(e2eMessageId).map { interactor?.deleteMessage(messageId: $0) }
            case "reply",
                 "edit",
                 "react": runMessageAction(action)
            case "dump": markMessages()
            case "retry-room":
                e2eReadyWaitStart = .now
                interactor?.retryRoom()
            case "frames": measureFrames(seconds: action.argument.flatMap(Double.init) ?? 3)
            case "search-open": showSearch()
            case "room-mute": applyUserState(.mute(Chat.MuteDuration(e2eName: action.argument)))
            case "room-unmute": applyUserState(.unmute)
            case "room-archive": applyUserState(.archive(true))
            case "room-unarchive": applyUserState(.archive(false))
            case "bottom":
                messages.values.max { ($0.order, $0.timestamp) < ($1.order, $1.timestamp) }
                    .map { view?.scrollToMessage($0.messageId) }
            case "media": runMedia(message: action.target ?? "last", index: action.argument.flatMap(Int.init) ?? 0)
            case "show-more": action.target.flatMap(e2eMessageId).map { expandText(of: $0) }
            default:
                if !runCallE2E(action) { logger.warning("[e2e:ui] unknown action \(action.action)") }
            }
        }

        /// The actions of the message menu that the harness asks for: a reply with the text of the argument to
        /// the target, an edit of the target to the text of the argument, or a reaction with the emoji of the
        /// argument (a second reaction with the same emoji removes it). They take the path of the room screen.
        private func runMessageAction(_ action: TrUAPIE2EHostAction) {
            guard let messageId = action.target.flatMap(e2eMessageId), let argument = action.argument else {
                mark(action.action, "\(action.target ?? "") missing")
                return
            }

            switch action.action {
            case "reply": interactor?.send(text: argument, attachments: nil, replyToMessageId: messageId)
            case "edit": interactor?.sendEdit(messageId: messageId, newText: argument)
            default: interactor?.toggleReaction(messageId: messageId, emoji: argument)
            }
        }

        /// Writes the messages of the window, oldest first, as one JSON array: the id that the product gave, the
        /// author (`user`, `product` or the participant id), the name that the room shows for the author, the
        /// time, the status, the text, the quoted message, the emoji of the reactions, and `markdown` for a text
        /// that the room draws in the Markdown subset.
        private func markMessages() {
            let rows: [[String: Any]] = messages.values
                .sorted { ($0.order, $0.timestamp) < ($1.order, $1.timestamp) }
                .map { message in
                    var row: [String: Any] = [
                        "id": productMessageId(message.messageId),
                        "status": message.status.e2eName,
                        "time": message.timestamp,
                        "text": message.content.originalMessageText ?? "",
                        "reactions": message.reactions.map(\.emoji)
                    ]
                    let author = message.authorParticipantId
                        ?? (message.status.isOutgoing ? "user" : Chat.ProductParticipant.product)
                    row["author"] = author
                    if let participant = message.authorParticipantId.flatMap(participants.participant) {
                        row["name"] = participant.e2eName
                    }
                    if case let .reply(reply) = message.content {
                        row["replyTo"] = productMessageId(reply.messageId)
                    }
                    if message.hasMarkdownText {
                        row["markdown"] = true
                    }
                    // The media of the message: the type and the size of each inline item, so that the harness
                    // sees a preview change into the file.
                    if case let .productBody(.richText(richText)) = message.content, !richText.media.isEmpty {
                        row["media"] = richText.media.map { source in
                            InlineDataURL(source).map { "\($0.mimeType) \($0.data.count)" } ?? "ref"
                        }
                    }
                    return row
                }
            let data = (try? JSONSerialization.data(withJSONObject: rows, options: [.sortedKeys])) ?? Data()
            e2eMarks["dump"] = nil
            mark("dump", String(bytes: data, encoding: .utf8) ?? "[]")
        }

        /// A tap on the media item at `index` of a message (PARITY row 4). The marker `presented` names the screen
        /// that the room shows after the tap.
        private func runMedia(message: String, index: Int) {
            guard let messageId = e2eMessageId(message), let request = mediaRequest(of: messageId, at: index) else {
                mark("media", "\(message) \(index) missing")
                return
            }

            Task { [weak self] in
                guard let self, await openMedia(request) else { return }
                try? await Task.sleep(for: .seconds(1))
                mark("presented", mediaViewer().map { String(describing: type(of: $0)) } ?? "none")
            }
        }

        /// The screen in front of the room screen: a presented viewer, or the document preview that the navigation
        /// pushes.
        private func mediaViewer() -> UIViewController? {
            guard let controller = view?.controller else { return nil }
            if let presented = controller.presentedViewController {
                return presented
            }
            let top = controller.navigationController?.topViewController
            return top === controller ? nil : top
        }

        private func dismissMediaViewer() {
            guard let controller = view?.controller else { return }
            if let presented = controller.presentedViewController {
                presented.dismiss(animated: false)
            } else if controller.navigationController?.topViewController !== controller {
                controller.navigationController?.popToViewController(controller, animated: false)
            }
            mark("presented", "none")
        }

        /// Deletes the outbox entries of the room, so that a row starts with none, also after a run that stopped.
        private func clearOutbox() {
            Task { [weak self, staging, roomId, appearanceFactory] in
                for entry in await staging.stager.entries(productId: appearanceFactory.productId, roomId: roomId) {
                    await staging.stager.delete(entry.id)
                }
                self?.mark("outbox", "cleared")
            }
        }

        /// B7: the time that 8 MiB takes to cross the binding into the core, and back with the refusal of a room
        /// that declares nothing. One byte gives the time of the call without the bytes.
        private func measureStagingCrossing(runs: Int) {
            let productId = appearanceFactory.productId
            Task { [weak self] in
                let staging = TrUAPIProductAttachmentStaging()
                var results: [String] = []
                for size in [1, ProductAttachmentStagingPolicy.coreMaxBytes] {
                    let bytes = Data(count: size)
                    var samples: [Double] = []
                    var outcome = "staged"
                    for _ in 0 ..< runs {
                        let start = ContinuousClock.now
                        do {
                            _ = try await staging.stage(
                                productId: productId,
                                roomId: "b7-probe",
                                mimeType: "application/octet-stream",
                                bytes: bytes
                            )
                        } catch let failure as ProductStageFailure {
                            outcome = failure.summary.replacingOccurrences(of: " ", with: "-")
                        } catch {
                            outcome = "error"
                        }
                        let elapsed = start.duration(to: .now)
                        samples.append(
                            Double(elapsed.components.seconds) * 1_000 + Double(elapsed.components.attoseconds) / 1e15
                        )
                    }
                    let sorted = samples.sorted()
                    results.append(String(
                        format: "bytes=%d median=%.1f min=%.1f max=%.1f outcome=%@",
                        size,
                        sorted[sorted.count / 2],
                        sorted.first ?? 0,
                        sorted.last ?? 0,
                        outcome
                    ))
                }
                self?.mark("b7", "runs=\(runs) " + results.joined(separator: " | "))
            }
        }

        /// Measures the frames of the room screen for some seconds, for example while the product streams edits
        /// (B3). The marker also gives how many window updates the room screen drew in that time.
        private func measureFrames(seconds: Double) {
            let meter = TrUAPIE2EFrameMeter()
            let updatesBefore = e2eUpdates
            e2eMeter = meter
            meter.measure(for: .milliseconds(Int(seconds * 1_000))) { [weak self] meter in
                guard let self else { return }
                e2eMeter = nil
                e2eMarks["frames"] = nil
                mark("frames", "\(meter.summary) updates=\(e2eUpdates - updatesBefore)")
            }
        }

        private func runMenu(message: String, select actionId: String?) {
            guard let messageId = e2eMessageId(message) else {
                mark("menu", "\(message) missing")
                return
            }

            Task {
                let answer = await messageMenu(for: messageId)
                let entries = ChatMessageBubbleAction.productMenu(
                    hostActions: e2eHostActions(messageId),
                    answer: answer
                )
                mark("menu", "\(productMessageId(messageId)) entries=" + entries.map(\.e2eName).joined(separator: ","))

                if let action = entries.productActions.first(where: { $0.actionId == actionId }) {
                    select(action, messageId: messageId)
                } else if actionId == "readAloud", entries.contains(.readAloud), let message = messages[messageId] {
                    readAloud(message.content.readAloudText, messageId: messageId)
                }
            }
        }

        /// The host entries of a message as the room screen draws them for a text message.
        private func e2eHostActions(_ messageId: Chat.MessageId) -> [ChatMessageBubbleAction] {
            guard let message = messages[messageId] else { return [] }
            return ChatMessageBubbleAction.textMessageActions(
                for: message.status,
                input: policy?.footer.showsInput == true ? .inputField(.init(canPay: false, canAttachFile: false))
                    : .empty,
                isEdited: false,
                offersRetry: policy?.offersRetry ?? false,
                offersDelete: policy?.offersDelete ?? false
            ) + (offersReadAloud ? [.readAloud] : [])
        }

        private func confirmPendingLink() {
            let onOpen = TrUAPIE2EPendingConfirmation.onOpen
            TrUAPIE2EPendingConfirmation.onOpen = nil
            view?.controller.presentedViewController?.dismiss(animated: false)
            onOpen?()
        }

        private func runParticipant(_ participantId: String?) {
            guard let participantId else { return }

            Task {
                if participants.participant(participantId) == nil {
                    await participants.resolveNow([participantId])
                }
                let participant = participants.participant(participantId)
                    ?? ChatRoomParticipant(
                        participantId: participantId,
                        kind: .custom(nickname: participantId, avatar: nil)
                    )
                let entries = participantSheetEntries(for: participant)
                mark("sheet", "\(participantId) entries=" + entries.map(\.e2eName).joined(separator: ","))
                didSelectParticipant(participant)
            }
        }

        private func runAttach(path: String?, kind: String?) {
            guard let path, let data = FileManager.default.contents(atPath: path) else {
                mark("attach", "unreadable")
                return
            }

            switch kind {
            case "image": sendAttachment(.image(data))
            case "video": sendAttachment(.video(data, mimeType: "video/mp4"))
            default: sendAttachment(.file(data, name: (path as NSString).lastPathComponent))
            }
        }
    }

    private extension ChatMessageBubbleAction {
        var e2eName: String {
            switch self {
            case .reply: "reply"
            case .edit: "edit"
            case .copy: "copy"
            case .reaction: "react"
            case .edited: "edited"
            case .retry: "retry"
            case .delete: "delete"
            case let .product(action): "product:" + action.actionId + (action.destructive ? "!" : "")
            case .readAloud: "readAloud"
            }
        }
    }

    private extension ChatRoomParticipant {
        /// The name that the room shows for the participant, for the harness.
        var e2eName: String {
            switch kind {
            case let .custom(nickname, _): nickname
            default: String(describing: kind)
            }
        }
    }

    private extension ChatParticipantSheetEntry {
        var e2eName: String {
            switch self {
            case .copyAddress: "copyAddress"
            case .openProduct: "openProduct"
            }
        }
    }
#endif

extension Chat.LocalMessage.Status {
    /// The status as the harness reads it.
    var e2eName: String {
        switch self {
        case let .incoming(status): "in-" + status.rawValue
        case let .outgoing(status): "out-" + status.rawValue
        }
    }
}
