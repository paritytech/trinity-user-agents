import Foundation
import UIKitExt

final class ChatSampleExtension: ChatExtensionBot {
    let logger: LoggerProtocol

    init(logger: LoggerProtocol) {
        self.logger = logger
    }

    override func onTextMessage(
        _: Chat.LocalMessage,
        text: String,
        context: ChatExtensionProcessingContextProtocol
    ) async -> ChatExtension.ProcessingResult {
        Task {
            let newText = "Echo: \(text)"

            do {
                try await context.addNewMessage(
                    .text(newText),
                    delayDelivery: .humanInteraction,
                    chatExtension: self
                )
            } catch {
                logger.error("Can't handle text message: \(error)")
            }
        }

        return .processed
    }
}

extension ChatSampleExtension: ChatExtensionBotProtocol {
    var identifier: ChatExtension.Id { "EchoBot" }

    var peerMetadata: Chat.PeerMetadata {
        Chat.PeerMetadata(
            name: "Sample Echo Bot",
            icon: .image(nil),
            input: .inputField(.init(canPay: false, canAttachFile: true)),
            moreActions: []
        )
    }

    func deliverAutomaticMessages(_ context: ChatExtensionDiscoverContextProtocol) {
        Task {
            do {
                try await context.setWelcomeMessages(
                    from: self,
                    with: { [.text("Hello! Do you want to chat?")] }
                )
            } catch {
                self.logger.error("Can't set welcome message: \(error)")
            }
        }
    }

    func process(action _: Chat.Action, context _: ChatExtensionActionContextProtocol) async {
        // no actions for echo bot
    }

    func attach(presentationView _: ControllerBackedProtocol) {
        // no additional presentation
    }
}
