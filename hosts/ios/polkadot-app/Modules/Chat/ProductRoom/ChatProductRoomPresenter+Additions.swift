import Foundation
import PolkadotUI
import UIKit

/// The Chat Extension v2 additions of a product room screen (07-LATER §5, §9): the personas of the product and
/// read aloud.
extension ChatProductRoomPresenter {
    /// Follows the personas of the product. Each change draws the room again.
    func followAdditions() {
        if let personaStore = additions.dependencies.personaStore, case let .chatExtension(productId, _) = chatId {
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

    // MARK: - Long texts

    /// True when the user expanded the long text of the message with "Show more" (02-DESIGN §6.1).
    func isTextExpanded(_ messageId: Chat.MessageId) -> Bool {
        additions.expandedTexts.contains(productMessageId(messageId))
    }

    /// Shows the whole text of the message. The text stays expanded while the room screen is open.
    func expandText(of messageId: Chat.MessageId) {
        let productId = productMessageId(messageId)
        guard additions.expandedTexts.insert(productId).inserted else { return }
        mark("text-" + productId, "expanded")
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

    /// The room screen appears.
    func screenWillAppear() {
        backgroundObserver?.screenDidAppear()
        if let readAloud = additions.dependencies.readAloud {
            bindReadAloud(readAloud)
        }
    }

    /// The user leaves the room screen: read aloud stops (07-LATER §9.5).
    func screenWillDisappear() {
        backgroundObserver?.screenDidDisappear()
        stopReadAloud()
    }
}

// MARK: - Private

extension ChatProductRoomPresenter {
    /// Sends a room event of the user to the product, for example a call event. The core keeps
    /// the event until the worker of the product subscribes. A lost event is logged.
    func publishRoomEvent(_ event: Chat.RoomEvent) {
        guard
            let publisher = additions.dependencies.eventPublisher ?? roomEvents,
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

    private func didReceive(personas: [Chat.ProductPersona]) async {
        additions.personas = Dictionary(personas.map { ($0.botId, $0) }, uniquingKeysWith: { _, newest in newest })
        mark("personas", personas.map { "\($0.botId)=\($0.name)" }.joined(separator: ","))
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

    private func markPersonaIcons() {
        mark("persona-icons", additions.personaIcons.keys.sorted().joined(separator: ","))
    }
}
