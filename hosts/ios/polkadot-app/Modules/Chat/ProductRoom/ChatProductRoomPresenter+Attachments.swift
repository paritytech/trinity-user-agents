import Foundation

/// An attachment that the user picked, before the host makes it inline.
enum PickedAttachment: Equatable {
    case image(Data)
    case video(Data, mimeType: String)
    case file(Data, name: String)
}

/// The attachments of a product room (P6.7, scenario S36): inline up to 1 MiB, with their metadata. A larger
/// attachment is refused with a message, because this host has no preimage path yet (P10).
extension ChatProductRoomPresenter {
    /// Offers the kinds of attachment that the input of the room allows.
    func showAttachmentPicker() {
        guard case let .input(input)? = policy?.footer else { return }

        let kinds = input.attachmentKinds
        switch kinds.count {
        case 0:
            return
        case 1:
            pickAttachment(kinds[0])
        default:
            wireframe.chooseAttachmentKind(kinds, from: view) { [weak self] kind in
                self?.pickAttachment(kind)
            }
        }
    }

    func pickAttachment(_ kind: ChatAttachmentKind) {
        wireframe.pickAttachment(kind, from: view) { [weak self] picked in
            self?.sendAttachment(picked)
        }
    }

    /// Makes the attachment inline and sends it as a message of the user. In a room that the host stores, the
    /// product receives `MessagePosted`. In a room that the product keeps, it receives `NewMessage`.
    func sendAttachment(_ picked: PickedAttachment) {
        let attachment: ProductAttachment
        do {
            attachment = try Self.encode(picked)
        } catch let ProductAttachmentError.tooLarge(bytes) {
            logger.debug("Attachment of \(bytes) bytes refused in room \(roomId)")
            view?.showActionFailure(String(localized: .ChatExtension.attachmentTooLarge))
            return
        } catch {
            view?.showActionFailure(String(localized: .ChatExtension.attachmentUnreadable))
            return
        }

        guard let sendContent else { return }
        Task { [weak self] in
            do {
                try await sendContent(attachment.content)
            } catch let refusal as ChatUserActionRefusal {
                self?.view?.showActionFailure(refusal.reason ?? String(localized: .ChatExtension.actionRefused))
            } catch {
                self?.logger.error("Attachment of room \(self?.roomId ?? "") not sent: \(error)")
            }
        }
    }

    static func encode(_ picked: PickedAttachment) throws -> ProductAttachment {
        switch picked {
        case let .image(data): try ProductAttachmentEncoder.image(data)
        case let .video(data, mimeType): try ProductAttachmentEncoder.video(data, mimeType: mimeType)
        case let .file(data, name): try ProductAttachmentEncoder.file(data, fileName: name)
        }
    }
}
