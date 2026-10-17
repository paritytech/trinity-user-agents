import Foundation
import PolkadotUI
import Testing
import TrUAPIHost
@testable import polkadot_app

/// The quick replies, the slash command catalogue, the activity line and the placeholder of a product room
/// (P6.8, scenarios S37 to S40).
@MainActor
struct ChatProductExtrasTests {
    private typealias Data = ProductRoomTestData

    private let room = RecordingProductRoom()
    private let wireframe = RecordingProductRoomWireframe()
    private let activity = ChatRoomActivityStore()

    // MARK: - Suggestions (S38)

    @Test func theFooterShowsTheQuickRepliesAndTheCommandsOfTheRoom() throws {
        let presenter = try makePresenter()

        #expect(presenter.accessory.suggestions.map(\.suggestionId) == ["sg-send", "sg-event"])
        #expect(presenter.accessory.commands.map(\.name) == ["help", "post"])
        #expect(presenter.accessory.commands.last?.argumentHint == "<kind>")
    }

    @Test func aSendsTextChipSendsItsTextAsAMessage() async throws {
        let presenter = try makePresenter()
        var sent: [String] = []
        presenter.sendText = { sent.append($0) }

        presenter.accessory.onSuggestion?(.init(suggestionId: "sg-send", text: "Say hello"))
        try await Task.sleep(for: .milliseconds(20))

        #expect(sent == ["Say hello"])
        #expect(room.calls.events.isEmpty)
    }

    @Test func anEventChipSendsSuggestionSelectedOnly() async throws {
        let presenter = try makePresenter()
        var sent: [String] = []
        presenter.sendText = { sent.append($0) }

        presenter.selectSuggestion("sg-event")
        await room.waitForEvents(1)

        #expect(room.events == [.suggestionSelected(suggestionId: "sg-event")])
        #expect(sent.isEmpty)
    }

    // MARK: - Input events

    /// A room that declares `input_events` gets the text once the user stops typing, not each key, and the quick
    /// replies stay while the user types.
    @Test func aRoomThatFollowsTheInputGetsTheTextAfterAPause() async throws {
        let presenter = try makePresenter(inputEvents: true)
        #expect(presenter.accessory.followsInput)

        presenter.accessory.onInputText?("a")
        presenter.accessory.onInputText?("al")
        presenter.accessory.onInputText?("ali")
        await room.waitForEvents(1)
        try await Task.sleep(for: .milliseconds(400))
        presenter.accessory.onInputText?("ali")
        presenter.accessory.onInputText?(String(repeating: "a", count: 257))
        presenter.accessory.onInputText?("")
        await room.waitForEvents(2)

        #expect(room.events == [.inputChanged(text: "ali"), .inputChanged(text: "")])
    }

    @Test func aRoomThatDoesNotFollowTheInputGetsNoText() async throws {
        let presenter = try makePresenter()
        #expect(!presenter.accessory.followsInput)

        presenter.accessory.onInputText?("ali")
        try await Task.sleep(for: .milliseconds(400))

        #expect(room.events.isEmpty)
    }

    // MARK: - Commands (S39)

    @Test func theCatalogueMatchesTheTypedPrefixOnly() {
        let model = ChatRoomAccessoryModel()
        model.commands = [
            .init(name: "help", description: "", argumentHint: nil),
            .init(name: "history", description: "", argumentHint: "<n>"),
            .init(name: "post", description: "", argumentHint: "<kind>")
        ]

        #expect(model.matchingCommands(for: "/").map(\.name) == ["help", "history", "post"])
        #expect(model.matchingCommands(for: "/h").map(\.name) == ["help", "history"])
        #expect(model.matchingCommands(for: "/post text").isEmpty)
        #expect(model.matchingCommands(for: "hello").isEmpty)
    }

    // MARK: - Activity (S40)

    @Test func theActivityLineFollowsTheStoreAndClearsWithIt() async throws {
        let presenter = try makePresenter()

        activity.set(.init(text: "Sample is typing", participantId: nil), for: Data.chatId)
        try await waitUntil { presenter.accessory.activityText == "Sample is typing" }

        activity.set(nil, for: Data.chatId)
        try await waitUntil { presenter.accessory.activityText == nil }
    }

    @Test func aChangeOfTheAccessoryReachesItsObservers() {
        let model = ChatRoomAccessoryModel()
        var changes = 0
        let observation = model.observe { changes += 1 }

        model.activityText = "Typing"
        model.activityText = "Typing"
        model.suggestions = [.init(suggestionId: "a", text: "A")]

        #expect(changes == 2)
        _ = observation
    }

    // MARK: - Placeholder (S37)

    @Test func thePlaceholderWaitsForTheWholeRoom() {
        var model = MessageListModel.stub([])
        #expect(model.showsPlaceholder)

        model.paging = ChatPagingState(older: .loading, newer: .end)
        #expect(!model.showsPlaceholder)

        model.paging = .complete
        model.readiness = .loading
        #expect(!model.showsPlaceholder)
    }
}

private extension ChatProductExtrasTests {
    func makePresenter(inputEvents: Bool = false) throws -> ChatProductRoomPresenter {
        let presenter = try #require(ChatProductRoomPresenter(
            chatId: Data.chatId,
            room: room,
            wireframe: wireframe,
            resolver: NoImageResolver(),
            activityStore: activity,
            logger: StubLogger()
        ))
        presenter.update(
            policy: ChatRoomPolicy(footer: nil, declarations: Data.declarations(
                suggestions: [
                    .init(suggestionId: "sg-send", text: "Say hello", sendsText: true),
                    .init(suggestionId: "sg-event", text: "Show help", sendsText: false)
                ],
                commands: [
                    .init(name: "help", description: "Lists the commands", argumentHint: nil),
                    .init(name: "post", description: "Posts a body", argumentHint: "<kind>")
                ]
            )).with(features: Chat.RoomFeatures(
                preimageAttachments: false,
                search: false,
                calls: nil,
                threadOf: nil,
                readAloud: false,
                inputEvents: inputEvents
            )),
            messages: []
        )
        return presenter
    }

    func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0 ..< 200 where !condition() {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(condition())
    }
}

private extension ChatRoomPolicy {
    func with(features: Chat.RoomFeatures) -> ChatRoomPolicy {
        var policy = self
        policy.features = features
        return policy
    }
}
