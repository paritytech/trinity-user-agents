import Foundation
import Testing

@testable import polkadot_app

@Suite("Chat message list sections")
struct ChatInteractorStateSectionsTests {
    private let calendar = Calendar.current
    private let contactId = Data(repeating: 0x01, count: 32)

    @Test("a late message from an earlier day folds into the current section without a time")
    func lateMessageFoldsIntoCurrentSection() async {
        let state = ChatInteractorState()
        let messages = [
            makeMessage(id: "a1", daysAgo: 3),
            makeMessage(id: "b1", daysAgo: 1),
            makeMessage(id: "a2", daysAgo: 3)
        ]

        let model = await state.onMessagesUpdate(messages)

        #expect(model.orderedSections.count == 2)
        #expect(model.messagesBySection[model.orderedSections[0]]?.map(\.messageId) == ["a1"])
        #expect(model.messagesBySection[model.orderedSections[1]]?.map(\.messageId) == ["b1", "a2"])
        #expect(model.outOfDayMessageIds == ["a2"])
    }

    @Test("messages arriving in day order keep one section per day and show their time")
    func inOrderMessagesKeepSectionsPerDay() async {
        let state = ChatInteractorState()
        let messages = [
            makeMessage(id: "a1", daysAgo: 3),
            makeMessage(id: "a2", daysAgo: 3),
            makeMessage(id: "b1", daysAgo: 1),
            makeMessage(id: "c1", daysAgo: 0)
        ]

        let model = await state.onMessagesUpdate(messages)

        #expect(model.orderedSections.count == 3)
        #expect(model.messagesBySection[model.orderedSections[0]]?.map(\.messageId) == ["a1", "a2"])
        #expect(model.messagesBySection[model.orderedSections[1]]?.map(\.messageId) == ["b1"])
        #expect(model.messagesBySection[model.orderedSections[2]]?.map(\.messageId) == ["c1"])
        #expect(model.outOfDayMessageIds.isEmpty)
    }

    @Test("sections never repeat whatever the arrival order")
    func sectionsNeverRepeat() async {
        let state = ChatInteractorState()
        let messages = [
            makeMessage(id: "a1", daysAgo: 3),
            makeMessage(id: "c1", daysAgo: 0),
            makeMessage(id: "a2", daysAgo: 3),
            makeMessage(id: "b1", daysAgo: 2),
            makeMessage(id: "c2", daysAgo: 0)
        ]

        let model = await state.onMessagesUpdate(messages)

        #expect(Set(model.orderedSections).count == model.orderedSections.count)
        #expect(model.orderedSections.count == 2)
        #expect(model.outOfDayMessageIds == ["a2", "b1"])
    }
}

private extension ChatInteractorStateSectionsTests {
    func makeMessage(id: Chat.MessageId, daysAgo: Int) -> Chat.LocalMessage {
        let noon = calendar.date(byAdding: .hour, value: 12, to: calendar.startOfDay(for: Date()))!
        let date = calendar.date(byAdding: .day, value: -daysAgo, to: noon)!

        return Chat.LocalMessage(
            messageId: id,
            chatId: .person(contactId),
            origin: .contact(contactId),
            creationSource: .localDevice,
            status: .incoming(.new),
            timestamp: date.toChatTimestamp(),
            content: .text(id),
            reactions: [],
            compactionId: nil,
            relatedMessages: []
        )
    }
}
