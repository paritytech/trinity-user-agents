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
        let config = Chat.RoomConfig(
            productSource: true,
            reactionDetails: false,
            participants: true,
            messageActions: false,
            participantActions: true,
            allowedReactions: ["👍"],
            placeholder: nil,
            reportSeen: true
        )

        let expected = Data([1, 0, 1, 0, 1]) + Data([1, 0x04, 0x10, 0xF0, 0x9F, 0x91, 0x8D]) + Data([0, 1])
        #expect(try config.scaleEncoded() == expected)
        #expect(try Chat.RoomConfig.fromScaleEncoded(expected) == config)
    }

    @Test func anInputFooterHasTheWireLayout() throws {
        let destination = Data(repeating: 0xAB, count: 32)
        let footer = Chat.RoomFooterConfig.input(Chat.RoomInputConfig(
            placeholder: "Ask",
            photo: true,
            video: false,
            file: true,
            payment: .enabled(destination: destination)
        ))

        let expected = Data([0, 1, 0x0C]) + Data("Ask".utf8) + Data([1, 0, 1, 1, 0]) + destination
        #expect(try footer.scaleEncoded() == expected)
        #expect(try Chat.RoomFooterConfig.fromScaleEncoded(expected) == footer)
        #expect(try Chat.RoomFooterConfig.empty.scaleEncoded() == Data([1]))
        #expect(try Chat.RoomFooterConfig.custom.scaleEncoded() == Data([2]))
    }

    @Test func aPaymentDestinationIsThirtyTwoBytes() {
        let footer = Chat.RoomFooterConfig.input(Chat.RoomInputConfig(
            placeholder: nil,
            photo: true,
            video: true,
            file: false,
            payment: .enabled(destination: Data([1, 2, 3]))
        ))

        #expect(throws: (any Error).self) { try footer.scaleEncoded() }
    }

    @Test func aToolbarHasTheWireLayout() throws {
        let toolbar = [
            Chat.RoomToolbarAction(
                actionId: "a",
                label: "L",
                icon: .inline(mimeType: "image/png", data: Data([1, 2])),
                handler: .deeplink(url: "x://y")
            ),
            Chat.RoomToolbarAction(actionId: "b", label: "M", icon: .preimage(key: Data([9])), handler: .callback)
        ]

        let first = Data([0x04, 0x61, 0x04, 0x4C, 0, 0x24]) + Data("image/png".utf8) + Data([0x08, 1, 2])
            + Data([0, 0x14]) + Data("x://y".utf8)
        let second = Data([0x04, 0x62, 0x04, 0x4D, 1, 0x04, 9, 1])
        let expected = Data([0x08]) + first + second
        #expect(try toolbar.scaleEncoded() == expected)
        #expect(try [Chat.RoomToolbarAction].fromScaleEncoded(expected) == toolbar)
    }

    @Test func suggestionsAndCommandsHaveTheWireLayout() throws {
        let suggestions = [Chat.RoomSuggestion(suggestionId: "s", text: "Hi", sendsText: true)]
        let commands = [Chat.RoomCommand(name: "roll", description: "Roll dice", argumentHint: "<n>")]

        #expect(try suggestions.scaleEncoded() == Data([0x04, 0x04, 0x73, 0x08, 0x48, 0x69, 1]))
        let expectedCommands = Data([0x04, 0x10]) + Data("roll".utf8) + Data([0x24]) + Data("Roll dice".utf8)
            + Data([1, 0x0C]) + Data("<n>".utf8)
        #expect(try commands.scaleEncoded() == expectedCommands)
        #expect(try [Chat.RoomCommand].fromScaleEncoded(expectedCommands) == commands)
    }

    /// Bytes that are not a complete value read as no declaration, so the room keeps the host defaults.
    @Test func bytesThatDoNotDecodeReadAsNotDeclared() async throws {
        let facade = UserDataStorageTestFacade()
        let declarations = try await facade.databaseService.performWrite { context in
            let entity = try context.insertNew(CDChatRoomMetadata.self)
            entity.config = Data([1, 0])
            entity.footerConfig = Data([7])
            entity.commands = try [Chat.RoomCommand(name: "a", description: "b", argumentHint: nil)].scaleEncoded()
                + Data([0])
            entity.suggestions = Data()
            let declarations = Chat.RoomDeclarations(entity: entity)
            context.rollback()
            return declarations
        }

        #expect(declarations.config == nil)
        #expect(declarations.footerConfig == nil)
        #expect(declarations.commands == nil)
        #expect(declarations.suggestions == nil)
        #expect(declarations.effectiveConfig == .hostDefault)
    }
}
