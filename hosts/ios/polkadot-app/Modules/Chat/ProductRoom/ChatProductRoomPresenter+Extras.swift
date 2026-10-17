import Foundation
import PolkadotUI
import TrUAPIHost

/// The quick replies, the slash command catalogue and the activity line of a product room (P6.8, 02-DESIGN §6.6,
/// scenarios S38 to S40).
extension ChatProductRoomPresenter {
    /// Gives the quick replies and the commands of the policy to the footer.
    func updateAccessory() {
        accessory.suggestions = (policy?.roomSuggestions ?? []).map {
            ChatRoomAccessoryModel.Suggestion(suggestionId: $0.suggestionId, text: $0.text)
        }
        accessory.commands = (policy?.roomCommands ?? []).map {
            ChatRoomAccessoryModel.Command(name: $0.name, description: $0.description, argumentHint: $0.argumentHint)
        }
        accessory.followsInput = policy?.features.inputEvents == true
        mark("suggestions", accessory.suggestions.map(\.suggestionId).joined(separator: ","))
        mark("commands", "count=\(accessory.commands.count)")
    }

    /// The text of the input, for a room that declares `input_events`. The product gets it after the user stops
    /// typing for a moment, and only while it fits the bound of the core. A text that the product saw last is not
    /// sent again.
    func inputTextChanged(_ text: String) {
        inputEventTask?.cancel()
        guard policy?.features.inputEvents == true else { return }
        let text = text.utf8.count <= Self.inputEventMaxBytes ? text : nil
        inputEventTask = Task { [weak self] in
            try? await Task.sleep(for: Self.inputEventDelay)
            guard let self, !Task.isCancelled, let text, text != lastInputEventText else { return }
            lastInputEventText = text
            publish(.inputChanged(text: text))
        }
    }

    /// A quick reply with `sends_text` sends its text as a message of the user. Another one sends
    /// `SuggestionSelected` only.
    func selectSuggestion(_ suggestionId: String) {
        guard let suggestion = policy?.roomSuggestions.first(where: { $0.suggestionId == suggestionId }) else {
            return
        }

        if suggestion.sendsText {
            sendText?(suggestion.text)
        } else {
            publish(.suggestionSelected(suggestionId: suggestion.suggestionId))
        }
    }

    /// Follows the activity line of the room. The product sets it, and the room clears it on `RoomClosed`.
    func followActivity(_ store: ChatRoomActivityStoring) -> Task<Void, Never> {
        Task { [weak self, chatId] in
            for await activity in store.activityUpdates(for: chatId) {
                guard let self else { return }
                additions.activity = activity
                showActivity()
            }
        }
    }

    /// Draws the activity line again, for example when the persona that it is about loads.
    func showActivity() {
        let line = activityLine(additions.activity)
        accessory.activityText = line.text
        accessory.activityIcon = line.icon
        mark("activity", (line.text ?? "none") + (line.icon == nil ? "" : " icon"))
    }
}
