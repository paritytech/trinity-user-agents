import Foundation
import Testing
import UIKit
@testable import polkadot_app

/// The toolbar of a product room (P6.2): the participants button of the host, when the room has participants.
@MainActor
struct ChatProductToolbarTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let view = RecordingProductRoomView()

    @Test func aRoomWithParticipantsShowsTheParticipantsButton() throws {
        _ = try makePresenter(participants: true)

        let items = try #require(view.toolbars.last)
        #expect(items.count == 1)
        #expect(items.first?.primaryAction?.title == String(localized: .ChatExtension.participantsButton))
    }

    @Test func aRoomWithoutParticipantsShowsNoButton() throws {
        _ = try makePresenter(participants: false)

        #expect(view.toolbars.last?.isEmpty == true)
    }
}

private extension ChatProductToolbarTests {
    func makePresenter(participants: Bool) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            logger: StubLogger()
        ))
        presenter.view = view
        presenter.update(policy: Data.policy(Data.config(participants: participants)), messages: [])
        return presenter
    }
}
