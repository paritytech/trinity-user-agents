import Foundation
import Operation_iOS

struct DeviceSyncIncomingMessageFilter {
    let repository: AnyDataProviderRepository<Chat.LocalMessage>
    let peerLogger: LoggerProtocol

    func filterMessagesToApply(
        in messages: [Chat.LocalMessage]
    ) async throws -> [Chat.LocalMessage] {
        var result: [Chat.LocalMessage] = []
        result.reserveCapacity(messages.count)

        for message in messages {
            let existingMessage = try await repository
                .fetchOperation(by: { message.messageId }, options: .init())
                .asyncExecute()

            guard let existingMessage else {
                result.append(message)
                continue
            }

            guard existingMessage.creationSource == .deviceSync else {
                continue
            }

            if existingMessage.isRequestMessage {
                peerLogger.warning("Skipped synced overwrite for local request message: \(message.messageId)")
                continue
            }

            result.append(message)
        }

        return result
    }
}

private extension Chat.LocalMessage {
    var isRequestMessage: Bool {
        switch content {
        case .chatRequest,
             .versionedChatRequest:
            true
        default:
            false
        }
    }
}
