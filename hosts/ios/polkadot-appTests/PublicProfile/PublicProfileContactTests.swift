import Foundation
import Testing
@testable import polkadot_app

struct PublicProfileContactTests {
    @Test
    func aContactWithoutAnInlineImageLoadsItsPublicProfile() {
        let contact = makeContact(username: "alice.01")

        #expect(contact.toPeerMetadata().icon == .publicProfile(username: "alice.01"))
    }

    @Test
    func anInlineContactImageKeepsPriorityOverThePublicProfile() {
        let image = Data([1, 2, 3])
        let contact = makeContact(username: "alice.01", image: image)

        #expect(contact.toPeerMetadata().icon == .image(image))
    }

    @Test
    func aContactWithoutAUsernameKeepsItsPlaceholder() {
        let contact = makeContact(username: "")

        #expect(contact.toPeerMetadata().icon == .image(nil))
    }
}

private extension PublicProfileContactTests {
    func makeContact(username: String, image: Data? = nil) -> Chat.Contact {
        Chat.Contact(
            accountId: Data(repeating: 1, count: 32),
            username: username,
            publicKey: Data(repeating: 2, count: 32),
            ownKeyId: .init(signKeyId: "sign", encryptionKeyId: "encryption"),
            imageData: image,
            isBlocked: false,
            devices: [],
            pendingDevicesFanOut: false
        )
    }
}
