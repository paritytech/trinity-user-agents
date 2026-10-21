import Foundation
import TrUAPIHost

// Maps the participants of `participants_query` to the Swift domain values of the room screen. As in
// `ChatExtensionV2+Domain.swift`, this file never names the resource type of the bindings.

extension ChatRoomParticipant {
    init(_ entry: ChatParticipantEntry) {
        let kind: Kind =
            switch entry.participant {
            case let .person(personId): .person(personId: personId)
            case let .litePerson(account): .litePerson(account: account)
            case let .contact(handle): .contact(handle: handle.bytes)
            case let .product(productId): .product(productId: productId)
            case let .custom(nickname, .inline(mimeType, bytes)?):
                .custom(nickname: nickname, avatar: .inline(mimeType: mimeType, data: bytes))
            case let .custom(nickname, .preimage(key)?):
                .custom(nickname: nickname, avatar: .preimage(key: key))
            case let .custom(nickname, .sealed?),
                 let .custom(nickname, .hop?):
                .custom(nickname: nickname, avatar: nil)
            case let .custom(nickname, nil):
                .custom(nickname: nickname, avatar: nil)
            }

        self.init(participantId: entry.participantId, kind: kind)
    }

    /// The entry as `participant_actions_query` carries it. A reserved participant has no entry.
    var wireEntry: ChatParticipantEntry? {
        let participant: ChatParticipant
        switch kind {
        case .user,
             .roomProduct:
            return nil
        case let .person(personId):
            participant = .person(personId: personId)
        case let .litePerson(account):
            participant = .litePerson(account: account)
        case let .contact(handle):
            participant = .contact(handle: ContactHandle(bytes: handle))
        case let .product(productId):
            participant = .product(productId: productId)
        case let .custom(nickname, avatar):
            participant = .custom(nickname: nickname, avatar: avatar.map { resource in
                switch resource {
                case let .inline(mimeType, data): .inline(mimeType: mimeType, bytes: data)
                case let .preimage(key): .preimage(key: key)
                }
            })
        }

        return ChatParticipantEntry(participantId: participantId, participant: participant)
    }
}
