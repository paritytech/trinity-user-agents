import Foundation
import PolkadotUI
import UIKit

/// The Chat Extension v2 additions of a product room screen (07-LATER §5, §9): the personas of the product, the meta
/// of the messages, the branch controls, the reasoning rows, read aloud and the thread rooms.
extension ChatProductRoomPresenter {
    /// Follows the personas, the meta and the `Preparing` messages of the room. Each change draws the room again.
    func followAdditions() {
        let dependencies = additions.dependencies

        if let metaStore = dependencies.metaStore {
            additions.tasks.append(Task { [weak self, chatId] in
                do {
                    for try await index in metaStore.metaUpdates(for: chatId) {
                        self?.didReceive(meta: index)
                    }
                } catch {
                    self?.logger.warning("Room \(chatId.rawRepresentation) stops to follow its meta: \(error)")
                }
            })
        }

        if let personaStore = dependencies.personaStore, case let .chatExtension(productId, _) = chatId {
            additions.tasks.append(Task { [weak self] in
                do {
                    for try await personas in personaStore.personaUpdates(of: productId) {
                        await self?.didReceive(personas: personas)
                    }
                } catch {
                    self?.logger.warning("Product \(productId) stops to follow its personas: \(error)")
                }
            })
        }

        if let preparingMessages = dependencies.preparingMessages {
            additions.tasks.append(Task { [weak self, chatId] in
                for await preparing in preparingMessages.preparingUpdates(for: chatId) {
                    guard let self else { return }
                    additions.preparing = preparing
                    onChange?()
                }
            })
        }
    }

    /// The meta of a message, or nil when the product set none.
    func meta(of messageId: Chat.MessageId) -> Chat.MessageMeta? {
        additions.meta.meta(for: productMessageId(messageId))
    }

    // MARK: - Personas

    /// The persona of a participant id `bot:<bot_id>`, or nil for another participant or a persona that the
    /// product did not register.
    func persona(of participantId: String) -> Chat.ProductPersona? {
        Chat.ProductPersona.botId(ofParticipant: participantId).flatMap { additions.personas[$0] }
    }

    /// The author of a message of a persona, with its name and icon, without a question to the product.
    func personaAuthor(of participantId: String) -> ChatMessageAuthor? {
        guard let persona = persona(of: participantId) else { return nil }

        mark("author-" + participantId, persona.name + (additions.personaIcons[persona.botId] == nil ? "" : " icon"))
        return ChatMessageAuthor(
            participantId: participantId,
            name: persona.name,
            avatar: personaAvatar(persona),
            onTap: nil
        )
    }

    func personaAvatar(_ persona: Chat.ProductPersona) -> AvatarViewModel {
        additions.personaIcons[persona.botId].map(AvatarViewModel.image)
            ?? .colored(text: ChatParticipantAppearanceFactory.initials(of: persona.name), colorSeed: persona.botId)
    }

    /// The text of the activity line: the name of the persona that the line is about, then the text.
    func activityLine(_ activity: Chat.RoomActivity?) -> (text: String?, icon: UIImage?) {
        guard let activity else { return (nil, nil) }

        guard let participantId = activity.participantId, let persona = persona(of: participantId) else {
            return (activity.text, nil)
        }

        return ("\(persona.name): \(activity.text)", additions.personaIcons[persona.botId])
    }

    // MARK: - Branches

    /// The arrow that waits for the product, for a message, or nil.
    func pendingBranchIndex(of messageId: Chat.MessageId) -> UInt16? {
        additions.pendingBranches[productMessageId(messageId)]?.index
    }

    /// Sends `BranchSelected` and waits for the product to move the position. Without an answer in the time of
    /// ``ChatRoomAdditionsDependencies/branchTimeout``, the control goes back.
    func selectBranch(of messageId: Chat.MessageId, index: UInt16) {
        let productId = productMessageId(messageId)
        guard additions.pendingBranches[productId] == nil else { return }

        publishRoomEvent(.branchSelected(messageId: productId, index: index))

        let timeout = additions.dependencies.branchTimeout
        let task = Task { [weak self] in
            try? await Task.sleep(for: timeout)
            guard !Task.isCancelled, let self, additions.pendingBranches[productId]?.index == index else { return }
            additions.pendingBranches[productId] = nil
            mark("branch-" + productId, "\(currentBranchText(productId)) reverted")
            onChange?()
        }
        additions.pendingBranches[productId] = .init(index: index, timeout: task)
        mark("branch-" + productId, "\(currentBranchText(productId)) pending=\(index)")
        onChange?()
    }

    // MARK: - Reasoning rows

    func isThinking(_ messageId: Chat.MessageId) -> Bool {
        additions.preparing.contains(productMessageId(messageId))
    }

    func isReasoningExpanded(_ messageId: Chat.MessageId) -> Bool {
        additions.expandedReasoning.contains(productMessageId(messageId))
    }

    func toggleReasoning(of messageId: Chat.MessageId) {
        let productId = productMessageId(messageId)
        if additions.expandedReasoning.remove(productId) == nil {
            additions.expandedReasoning.insert(productId)
        }
        onChange?()
    }

    // MARK: - Read aloud

    /// The room offers "Read aloud" on its messages: the product declared `read_aloud` (D33).
    var offersReadAloud: Bool {
        additions.dependencies.readAloud != nil && policy?.features.readAloud == true
    }

    /// Speaks the text of a message. A `RichText` speaks as plain text.
    func readAloud(_ text: String, messageId: Chat.MessageId) {
        guard offersReadAloud, let readAloud = additions.dependencies.readAloud else { return }

        bindReadAloud(readAloud)
        let spoken = ChatReadAloudService.plainText(fromMarkdown: text)
        readAloud.speak(spoken, messageId: messageId, chatId: chatId)
        mark("readaloud", "speaking " + spoken.prefix(60).replacingOccurrences(of: "\n", with: " "))
    }

    func stopReadAloud() {
        additions.dependencies.readAloud?.stop(in: chatId)
    }

    // MARK: - Screen

    /// The room screen appears: it shows the root of the thread that the user comes back from, and a thread room
    /// puts its parent room below it.
    func screenWillAppear() {
        backgroundObserver?.screenDidAppear()
        additions.isOnScreen = true
        if let readAloud = additions.dependencies.readAloud {
            bindReadAloud(readAloud)
        }

        placeParentBelowIfNeeded()
        returnToRootIfShown()
    }

    /// The user leaves the room screen: read aloud stops (07-LATER §9.5).
    func screenWillDisappear() {
        backgroundObserver?.screenDidDisappear()
        additions.isOnScreen = false
        stopReadAloud()
    }
}

// MARK: - Private

extension ChatProductRoomPresenter {
    func publishRoomEvent(_ event: Chat.RoomEvent) {
        guard
            let publisher = additions.dependencies.eventPublisher,
            case let .chatExtension(productId, _) = chatId
        else {
            logger.warning("Room \(roomId) has no publisher for \(event)")
            return
        }

        do {
            try publisher.publish(event, productId: productId, roomId: roomId)
        } catch {
            logger.error("Room \(roomId) did not send \(event): \(error)")
        }
    }

    private func didReceive(meta index: Chat.MessageMetaIndex) {
        additions.meta = index

        for (productId, pending) in additions.pendingBranches
            where index.meta(for: productId)?.branch?.index == pending.index {
            pending.timeout.cancel()
            additions.pendingBranches[productId] = nil
        }

        markMeta(index)
        onChange?()
    }

    private func didReceive(personas: [Chat.ProductPersona]) async {
        additions.personas = Dictionary(personas.map { ($0.botId, $0) }, uniquingKeysWith: { _, newest in newest })
        mark("personas", personas.map { "\($0.botId)=\($0.name)" }.joined(separator: ","))
        showActivity()
        onChange?()

        for persona in personas {
            var image: UIImage?
            if let resource = persona.icon.flatMap(ChatResource.init(source:)) {
                image = await resolver.image(for: resource)
            }
            guard additions.personas[persona.botId] == persona else { continue }
            additions.personaIcons[persona.botId] = image
        }

        markPersonaIcons()
        showActivity()
        onChange?()
    }

    private func bindReadAloud(_ readAloud: ChatReadAloudServicing) {
        readAloud.onChange = { [weak self] state in
            self?.didChangeReadAloud(state)
        }
        readAloud.onSpeechStart = { [weak self] in
            self?.mark("readaloud-audio", "started")
        }
        didChangeReadAloud(readAloud.state)
    }

    private func didChangeReadAloud(_ state: ChatReadAloudState) {
        switch state {
        case let .speaking(speakingChatId, _) where speakingChatId == chatId:
            accessory.readAloudStopTitle = String(localized: .ChatExtension.readAloudStop)
            accessory.onStopReadAloud = { [weak self] in self?.stopReadAloud() }
        default:
            if accessory.readAloudStopTitle != nil {
                mark("readaloud", "stopped")
                e2eMarks["readaloud-audio"] = nil
            }
            accessory.readAloudStopTitle = nil
        }
    }

    private func currentBranchText(_ productId: String) -> String {
        additions.meta.meta(for: productId)?.branch.map { "\($0.index)/\($0.count)" } ?? "none"
    }

    private func markMeta(_ index: Chat.MessageMetaIndex) {
        for productId in index.productMessageIds {
            guard let meta = index.meta(for: productId) else { continue }
            if let branch = meta.branch {
                let pending = additions.pendingBranches[productId].map { " pending=\($0.index)" } ?? ""
                mark("branch-" + productId, "\(branch.index)/\(branch.count)" + pending)
            }
            if let thread = meta.thread {
                mark(
                    "chip-" + productId,
                    "replies=\(thread.replyCount) room=\(thread.roomId) "
                        + "participants=\(thread.participantIds.joined(separator: ","))"
                )
            }
        }
    }

    /// The messages of the window that have meta, for the harness: `id:b<index>/<count>t<replies>r`, or `none`.
    func markDrawnMeta() {
        mark("meta-drawn", {
            let drawn = messages.values.compactMap { message -> String? in
                guard let meta = meta(of: message.messageId) else { return nil }
                let branch = meta.branch.map { "b\($0.index)/\($0.count)" } ?? ""
                let thread = meta.thread.map { "t\($0.replyCount)" } ?? ""
                let reasoning = meta.reasoning == nil ? "" : "r"
                return productMessageId(message.messageId) + ":" + branch + thread + reasoning
            }
            .sorted()
            return drawn.isEmpty ? "none" : drawn.joined(separator: ",")
        }())
    }

    private func markPersonaIcons() {
        mark("persona-icons", additions.personaIcons.keys.sorted().joined(separator: ","))
    }
}
