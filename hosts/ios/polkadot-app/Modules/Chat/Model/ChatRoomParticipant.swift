import Foundation

/// A participant of a product room (`ChatParticipantEntry`, 02-DESIGN §3.3.7).
struct ChatRoomParticipant: Equatable, Sendable {
    enum Kind: Equatable, Sendable {
        /// The user of the host: the reserved id `"user"`.
        case user
        /// The product of the room: the reserved id `"product"`.
        case roomProduct
        /// A verified person of the People chain.
        case person(personId: UInt32)
        /// A registered account that is not a person yet.
        case litePerson(account: Data)
        /// A contact that the user picked through the Contacts API.
        case contact(handle: Data)
        /// Another product.
        case product(productId: String)
        /// A participant whose appearance the product supplies.
        case custom(nickname: String, avatar: Chat.ProductResource?)
    }

    let participantId: String
    let kind: Kind

    /// The participant of a reserved id, which the host resolves without a question (02-DESIGN §3.3.7).
    static func reserved(_ participantId: String) -> ChatRoomParticipant? {
        switch participantId {
        case Chat.ProductParticipant.user: ChatRoomParticipant(participantId: participantId, kind: .user)
        case Chat.ProductParticipant.product: ChatRoomParticipant(participantId: participantId, kind: .roomProduct)
        default: nil
        }
    }
}

/// How the room screen draws a participant.
struct ChatParticipantAppearance: Equatable {
    let name: String
    /// What the list shows under the name: the kind of the participant.
    let detail: String
    /// The text of the placeholder avatar.
    let initials: String
    /// The image of the avatar, when the participant has one that the host loads.
    let image: ChatResource?
    /// The icon of a product, which the product icon factory loads by its domain.
    let productDomain: String?
}

/// Names and draws the participants of a product room (02-DESIGN §3.3.7). The host has no lookup for a person or a
/// contact yet, so a person shows "Person #id", a lite person a short address and a contact a short handle.
struct ChatParticipantAppearanceFactory {
    let myUsername: String
    let productName: String
    let productId: String

    func appearance(of participant: ChatRoomParticipant) -> ChatParticipantAppearance {
        switch participant.kind {
        case .user:
            make(myUsername, detail: String(localized: .ChatExtension.participantYou))
        case .roomProduct:
            make(productName, detail: String(localized: .ChatExtension.participantProduct), productDomain: productId)
        case let .person(personId):
            make(
                String(localized: .ChatExtension.participantPerson(String(personId))),
                detail: String(localized: .ChatExtension.participantKindPerson)
            )
        case let .litePerson(account):
            make(Self.short(account), detail: String(localized: .ChatExtension.participantKindLitePerson))
        case let .contact(handle):
            make(
                String(localized: .ChatExtension.participantContact(Self.short(handle))),
                detail: String(localized: .ChatExtension.participantKindContact)
            )
        case let .product(productId):
            make(productId, detail: String(localized: .ChatExtension.participantProduct), productDomain: productId)
        case let .custom(nickname, avatar):
            ChatParticipantAppearance(
                name: nickname,
                detail: String(localized: .ChatExtension.participantKindGuest),
                initials: Self.initials(of: nickname),
                image: avatar?.chatResource,
                productDomain: nil
            )
        }
    }

    /// The appearance of an id that the product did not resolve.
    func unresolved(_ participantId: String) -> ChatParticipantAppearance {
        make(participantId, detail: "")
    }

    static func initials(of name: String) -> String {
        String(name.first(where: { $0.isLetter || $0.isNumber }) ?? "?").uppercased()
    }

    /// The first and the last four hex digits of 32 bytes.
    static func short(_ bytes: Data) -> String {
        let hex = bytes.map { String(format: "%02x", $0) }.joined()
        guard hex.count > 12 else { return "0x" + hex }
        return "0x" + hex.prefix(4) + "…" + hex.suffix(4)
    }

    private func make(_ name: String, detail: String, productDomain: String? = nil) -> ChatParticipantAppearance {
        ChatParticipantAppearance(
            name: name,
            detail: detail,
            initials: Self.initials(of: name),
            image: nil,
            productDomain: productDomain
        )
    }
}
