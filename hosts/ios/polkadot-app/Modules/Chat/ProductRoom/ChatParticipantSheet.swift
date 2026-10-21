import Foundation

/// The entries of the sheet of one participant (P6.4, 02-DESIGN §4, scenario S31).
enum ChatParticipantSheetEntry: Equatable {
    /// Copies the address of a lite person.
    case copyAddress(String)
    /// Opens the App of a product.
    case openProduct(productId: String)
}

enum ChatParticipantSheet {
    /// The host entry for the kind of the participant. The host has no People profile or contact lookup for a
    /// handle yet, so a person and a contact have no host entry.
    static func builtIns(for participant: ChatRoomParticipant, productId: String) -> [ChatParticipantSheetEntry] {
        switch participant.kind {
        case let .litePerson(account):
            [.copyAddress(address(of: account))]
        case let .product(otherProductId):
            [.openProduct(productId: otherProductId)]
        case .roomProduct:
            [.openProduct(productId: productId)]
        case .user,
             .person,
             .contact,
             .custom:
            []
        }
    }

    /// The SS58 address of an account with the generic prefix, or its hex when the account is not 32 bytes.
    static func address(of account: Data) -> String {
        (try? account.toAddress(using: .substrate(42))) ?? "0x" + account.map { String(format: "%02x", $0) }.joined()
    }
}
