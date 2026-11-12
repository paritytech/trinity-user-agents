import Foundation
import Foundation_iOS
import Operation_iOS
import SubstrateSdk
import StructuredConcurrency
import ChainRegistry

final class NewMessageHandler {
    let pushId: String
    let messageHex: String
    let contactsService: ContactsLocalStorageServicing
    let messageDecoder: ChatPushMessageDecoding
    let messageRepository: AnyDataProviderRepository<Chat.LocalMessage>
    let unreadMessageCountService: UnreadMessageCountService

    init(
        pushId: String,
        messageHex: String,
        contactsService: ContactsLocalStorageServicing,
        messageDecoder: ChatPushMessageDecoding,
        messageRepository: AnyDataProviderRepository<Chat.LocalMessage>,
        unreadMessageCountService: UnreadMessageCountService
    ) {
        self.pushId = pushId
        self.messageHex = messageHex
        self.contactsService = contactsService
        self.messageDecoder = messageDecoder
        self.messageRepository = messageRepository
        self.unreadMessageCountService = unreadMessageCountService
    }
}

enum NewMessageHandlerError: Error {
    case noContact
    case unsupportedMessage
}

extension NewMessageHandler: PushNotificationHandling {
    func handle(
        completion: @escaping (NotificationContentResult) -> Void
    ) {
        Task {
            var badgeCount: Int?

            do {
                let contact = try await contactsService.getContact(byPushId: pushId)
                    .asyncExecute()
                    .mapOrThrow(NewMessageHandlerError.noContact)

                guard !contact.isBlocked else {
                    badgeCount = await calculateBadgeCount(unsavedMessageId: nil)
                    completion(.createBlockedContactResult(badgeCount: badgeCount))
                    return
                }

                let payload = try messageDecoder.decodeMessage(messageHex, for: contact)

                if let fullMessage = payload.fullMessage {
                    await save(fullMessage, for: contact)
                }

                badgeCount = await calculateBadgeCount(for: payload)

                let contentResult = try await makeContentResult(
                    for: payload,
                    contact: contact
                )
                completion(contentResult.withBadgeCount(badgeCount))
            } catch {
                let contentResult = NotificationContentResult.createUnsupportedResult(badgeCount: badgeCount)
                completion(contentResult)
            }
        }
    }
}

private extension NewMessageHandler {
    func makeContentResult(
        for payload: Chat.NotificationPayload,
        contact: Chat.Contact
    ) async throws -> NotificationContentResult {
        switch payload.versioned {
        case let .v1(.full(content)):
            try await makeContentResult(for: content.content, contact: contact)
        case let .v1(.stripped(.coinageSend(coinage))):
            try await NotificationContentResult(
                title: contact.username,
                body: makeBody(totalValue: coinage.totalValue, contact: contact, displayInfo: assetDisplayInfo()),
                accountId: contact.accountId
            )
        case let .v1(.stripped(stripped)):
            try await makeContentResult(for: stripped.regularContent, contact: contact)
        }
    }

    // swiftlint:disable:next cyclomatic_complexity
    func makeContentResult(
        for content: ChatRemoteMessageContent?,
        contact: Chat.Contact
    ) async throws -> NotificationContentResult {
        switch content {
        case let .text(string):
            NotificationContentResult(
                title: contact.username,
                body: string,
                accountId: contact.accountId
            )
        case let .send(content):
            try await NotificationContentResult(
                title: contact.username,
                body: makeBody(sendContent: content, contact: contact, displayInfo: assetDisplayInfo()),
                accountId: contact.accountId
            )
        case .contactAdded:
            NotificationContentResult(
                title: contact.username,
                body: String(localized: .contactAddedYou(username: contact.username)),
                accountId: contact.accountId
            )
        case let .reply(replyContent):
            NotificationContentResult(
                title: contact.username,
                body: replyContent.ownContent.text ?? "",
                accountId: contact.accountId
            )
        case let .reacted(reactionContent):
            NotificationContentResult(
                title: contact.username,
                body: String(localized: .reactedWithEmoji(emoji: reactionContent.emoji)),
                accountId: contact.accountId
            )
        case .reactionRemoved:
            NotificationContentResult(
                title: contact.username,
                body: String(localized: .reactionRemoved)
            )
        case let .edited(editedContent):
            NotificationContentResult(
                title: contact.username,
                body: String(localized: .messageEdited(text: editedContent.newContent.text ?? "")),
                accountId: contact.accountId
            )
        case .leftChat:
            NotificationContentResult(
                title: contact.username,
                body: String(localized: .peerLeftChat(username: contact.username)),
                accountId: contact.accountId
            )
        case .chatAccepted,
             .multiChatAccepted:
            NotificationContentResult(
                title: contact.username,
                body: String(localized: .chatRequestAccepted),
                accountId: contact.accountId
            )
        case let .richText(richText):
            NotificationContentResult(
                title: contact.username,
                body: makeBody(richText: richText),
                accountId: contact.accountId
            )
        case let .dataChannelOffer(content):
            NotificationContentResult(
                title: contact.username,
                body: makeBody(dataOfferContent: content)
            )
        case let .coinageSend(content):
            try await NotificationContentResult(
                title: contact.username,
                body: makeBody(totalValue: content.totalValue, contact: contact, displayInfo: assetDisplayInfo()),
                accountId: contact.accountId
            )
        case .token,
             .dataChannelAnswer,
             .dataChannelCandidates,
             .deviceAdded,
             .deviceRemoved,
             .dataChannelClosed,
             .compactedMessages:
            throw NewMessageHandlerError.unsupportedMessage
        case .none:
            NotificationContentResult(
                title: contact.username,
                body: String(localized: .unsupportedContent)
            )
        }
    }

    func calculateBadgeCount(for payload: Chat.NotificationPayload) async -> Int? {
        let unsavedMessageId = payload.fullMessage == nil ? payload.messageId : nil

        return await calculateBadgeCount(unsavedMessageId: unsavedMessageId)
    }

    func calculateBadgeCount(unsavedMessageId: Chat.MessageId?) async -> Int? {
        do {
            return try await unreadMessageCountService.totalUnreadBadgeMessageCount(
                unsavedMessageId: unsavedMessageId
            )
        } catch {
            assertionFailure("Failed to calculate unread message badge count: \(error)")
            return nil
        }
    }

    func assetDisplayInfo() async throws -> AssetBalanceDisplayInfo {
        let mapper = ChainModelMapper()
        let repository = SubstrateDataStorageFacade.shared
            .createRepository(mapper: AnyCoreDataMapper(mapper))
        let mainAsset = AppConfig.Assets.mainAsset
        let chain = try await repository.fetchOperation(
            by: { mainAsset.chainId },
            options: RepositoryFetchOptions()
        )
        .asyncExecute()

        let asset = chain?.assets.first(where: { $0.assetId == mainAsset.assetId })
        let displayInfo = asset?.digitalDollarDisplayInfo ?? .usd

        return displayInfo
    }

    func makeBody(
        sendContent: Chat.RemoteMessageContentV1.MessageContent.SendContent.Legacy,
        contact: Chat.Contact,
        displayInfo: AssetBalanceDisplayInfo
    ) -> String {
        let balanceFactory = AssetBalanceFormatterFactory()
        let decimalAmount = sendContent.amount.decimal(assetInfo: displayInfo)
        let formatter = balanceFactory.createTokenFormatter(for: displayInfo).value(for: .current)
        let amountString = formatter.stringFromDecimal(decimalAmount) ?? ""
        return String(localized: .contactSentYouAsset(username: contact.username)) + " \(amountString)"
    }

    func makeBody(
        richText: ChatRemoteMessageContent.RichText
    ) -> String {
        let displayText = richText.text ?? ""

        guard displayText.isEmpty else {
            return displayText
        }

        return String(localized: .mediaAttachment)
    }

    func makeBody(
        totalValue: Balance,
        contact: Chat.Contact,
        displayInfo: AssetBalanceDisplayInfo
    ) -> String {
        let formatterFactory = AssetBalanceFormatterFactory()
        let formatter = TransferAmountViewModelFactory(
            targetAssetInfo: displayInfo,
            formatterFactory: formatterFactory
        )
        let amountString = formatter.amount(from: totalValue)
        return String(localized: .contactSentYouAsset(username: contact.username)) + " \(amountString)"
    }

    typealias DataChannelOfferContent =
        Chat.RemoteMessageContentV1.MessageContent.DataChannelOfferContent

    func makeBody(dataOfferContent: DataChannelOfferContent) -> String {
        switch dataOfferContent.purpose {
        case .audio:
            .init(localized: .incomingAudioCall)
        case .video:
            .init(localized: .incomingVideoCall)
        }
    }

    func save(_ remoteMessage: Chat.RemoteMessage, for contact: Chat.Contact) async {
        let localMessage = Chat.LocalMessage(
            remote: remoteMessage,
            creationSource: .localDevice,
            status: .incoming(.new),
            contactId: contact.accountId
        )
        guard let localMessage else {
            return
        }

        do {
            try await messageRepository.saveOperation({ [localMessage] }, { [] }).asyncExecute()
        } catch {
            // Do nothing in case of failure
            assertionFailure()
        }
    }
}
