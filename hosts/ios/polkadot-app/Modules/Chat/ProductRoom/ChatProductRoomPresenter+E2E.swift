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
            guard action.room == roomId, view?.controller.view.window != nil else { return }
            logger.debug("[e2e:ui] action \(roomId) \(action.action) \(action.argument ?? "") \(action.target ?? "")")

            switch action.action {
            case "send": action.argument.map { sendText?($0) }
            case "toolbar": runToolbar(action.argument)
            case "menu": runMenu(message: action.target ?? "last", select: action.argument)
            case "confirm": confirmPendingLink()
            case "participants": showParticipants()
            case "participant": runParticipant(action.target, select: action.argument)
            case "reactions": action.target.flatMap(e2eMessageId).map { showReactionDetails(for: $0) }
            case "suggestion": action.argument.map { selectSuggestion($0) }
            case "type":
                let text = action.argument ?? ""
                mark("catalogue", accessory.matchingCommands(for: text).map(\.name).joined(separator: ","))
                NotificationCenter.default.post(
                    name: TrUAPIE2EHostAction.inputTextNotification,
                    object: action.argument ?? ""
                )
            case "attach": runAttach(path: action.argument, kind: action.target)
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
            case "retry-room":
                e2eReadyWaitStart = .now
                interactor?.retryRoom()
            case "frames": measureFrames(seconds: action.argument.flatMap(Double.init) ?? 3)
            case "bottom":
                messages.values.max { ($0.order, $0.timestamp) < ($1.order, $1.timestamp) }
                    .map { view?.scrollToMessage($0.messageId) }
            case "chip",
                 "branch",
                 "reasoning": runAddition(action)
            default: logger.warning("[e2e:ui] unknown action \(action.action)")
            }
        }

        /// The gestures of the Chat Extension v2 additions: a tap on a thread chip, on a branch arrow (the
        /// argument is the index of the alternative), or on a reasoning row.
        private func runAddition(_ action: TrUAPIE2EHostAction) {
            guard let messageId = action.target.flatMap(e2eMessageId) else {
                mark(action.action, "\(action.target ?? "") missing")
                return
            }

            switch action.action {
            case "chip":
                guard let thread = meta(of: messageId)?.thread else {
                    mark("chip", "\(productMessageId(messageId)) none")
                    return
                }
                openThread(thread, rootMessageId: messageId)
            case "branch":
                guard let index = action.argument.flatMap(UInt16.init) else { return }
                selectBranch(of: messageId, index: index)
            default:
                toggleReasoning(of: messageId)
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

        private func runToolbar(_ actionId: String?) {
            guard let button = policy?.toolbarActions.first(where: { $0.actionId == actionId }) else { return }
            pressToolbar(button)
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
                    readAloud(message.content.threadRootText, messageId: messageId)
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

        private func runParticipant(_ participantId: String?, select actionId: String?) {
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
                let entries = await participantSheetEntries(for: participant)
                mark("sheet", "\(participantId) entries=" + entries.map(\.e2eName).joined(separator: ","))

                if let actionId, let entry = entries.first(where: { $0.e2eName == "product:" + actionId }) {
                    select(entry, of: participant)
                } else {
                    didSelectParticipant(participant)
                }
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

    private extension ChatParticipantSheetEntry {
        var e2eName: String {
            switch self {
            case .copyAddress: "copyAddress"
            case .openProduct: "openProduct"
            case let .product(action): "product:" + action.actionId
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
