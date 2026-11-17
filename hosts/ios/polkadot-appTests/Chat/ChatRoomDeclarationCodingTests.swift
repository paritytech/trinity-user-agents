import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import SubstrateSdkExt
import Testing
@testable import polkadot_app

/// The store keeps the configuration of a room as the SCALE bytes of its Chat Extension v2 wire value. The
/// expected bytes follow the field order of `v01::ChatRoomConfig`.
struct ChatRoomDeclarationCodingTests {
    @Test func aConfigurationHasTheWireLayout() throws {
        let config = Chat.RoomConfig(productSource: true, reactionDetails: false, reportSeen: true)

        let expected = Data([1, 0, 1])
        #expect(try config.scaleEncoded() == expected)
        #expect(try Chat.RoomConfig.fromScaleEncoded(expected) == config)
    }

    /// Bytes that are not a complete value read as no declaration, so the room keeps the host defaults.
    @Test func bytesThatDoNotDecodeReadAsNotDeclared() async throws {
        let facade = UserDataStorageTestFacade()
        let declarations = try await facade.databaseService.performWrite { context in
            let entity = try context.insertNew(CDChatRoomMetadata.self)
            entity.config = Data([1, 0])
            let declarations = Chat.RoomDeclarations(entity: entity)
            context.rollback()
            return declarations
        }

        #expect(declarations.config == nil)
        #expect(declarations.effectiveConfig == .hostDefault)
    }

    /// The last message of a preview keeps its edit, which the preview line of the chat list shows.
    @Test func theLastMessageOfAPreviewKeepsItsEdit() throws {
        let message = Chat.RoomPreviewMessage(
            messageId: "m1",
            status: .incoming(.new),
            timestamp: 1_000,
            content: .text("hello"),
            relatedMessages: [
                Chat.RelatedLocalMessage(
                    messageId: "m1/edit",
                    timestamp: 2_000,
                    content: .edited(.init(messageId: "m1", newContent: .init(text: "hi", attachments: nil))),
                    status: .incoming(.new)
                )
            ]
        )

        let bytes = try message.storedBytes()

        #expect(bytes.first == Chat.AdditionStoredForm.version)
        #expect(Chat.RoomPreviewMessage.decodedIfStored(bytes) == message)
        #expect(Chat.RoomPreviewMessage.decodedIfStored(Data(bytes.dropFirst())) == nil)
    }
}
