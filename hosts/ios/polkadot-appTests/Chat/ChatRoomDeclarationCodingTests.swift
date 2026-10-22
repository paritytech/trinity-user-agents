import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency
import SubstrateSdk
import SubstrateSdkExt
import Testing
@testable import polkadot_app

/// The store keeps each declaration as the SCALE bytes of its Chat Extension v2 wire value. The expected
/// bytes follow the field and variant order of `v01::ChatRoomConfig`, `v02::ChatRoomFooter` and the others.
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
}
