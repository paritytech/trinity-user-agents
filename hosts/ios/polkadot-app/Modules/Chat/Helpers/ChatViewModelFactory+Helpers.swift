import Foundation
import PolkadotUI
import SubstrateSdk
import UIKit.UIImage

extension ChatMetadata {
    var chatContactInfo: ChatHeaderConfiguration {
        let avatarViewModel: AvatarViewModel = {
            if let image = peerMetadata.icon.image {
                return .image(image)
            }

            let prefix = String(peerMetadata.name.prefix(1))
            return .colored(text: prefix, colorSeed: chatId.colorSeed)
        }()

        let info: String? = nil

        return ChatHeaderConfiguration(
            avatarViewModel: avatarViewModel,
            username: peerMetadata.name,
            additionalInfo: info
        )
    }
}

struct TransferBubbleProjection {
    let displayedValue: Balance
    let originalValue: Balance?
}

extension Chat.LocalMessage.Content.Transfer {
    var bubbleProjection: TransferBubbleProjection {
        let actualValue: Balance? =
            switch state {
            case let .incoming(incoming) where incoming.status == .claimed: incoming.actualValue
            case let .outgoing(outgoing) where outgoing.status == .claimed: outgoing.actualValue
            case .incoming,
                 .outgoing,
                 .none: nil
            }

        guard let actualValue, actualValue != totalValue else {
            return TransferBubbleProjection(displayedValue: totalValue, originalValue: nil)
        }
        return TransferBubbleProjection(displayedValue: actualValue, originalValue: totalValue)
    }

    /// `nil` is the initial state: the monitor has not written a row yet.
    var incomingViewState: ChatTransferMessageConfiguration.IncomingState {
        guard case let .incoming(incoming) = state else { return .detecting }
        return incoming.status.viewState
    }

    var outgoingViewState: ChatTransferMessageConfiguration.OutgoingState {
        guard case let .outgoing(outgoing) = state else { return .sending }
        return outgoing.status.viewState
    }
}

private extension IncomingTransferState.Status {
    var viewState: ChatTransferMessageConfiguration.IncomingState {
        switch self {
        case .detecting: .detecting
        case .claiming: .claiming
        case .claimed: .claimed
        case .failed: .failed
        }
    }
}

private extension OutgoingTransferState.Status {
    var viewState: ChatTransferMessageConfiguration.OutgoingState {
        switch self {
        case .sending: .sending
        case .sent: .sent
        case .claimed: .claimed
        case .failed: .failed
        }
    }
}

extension UIAction {
    static func chatReply(_ action: @escaping () -> Void) -> UIAction {
        UIAction(
            title: String(localized: .chatReply),
            image: UIImage(resource: .reply).withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
        ) { _ in
            action()
        }
    }

    static func chatEdit(_ action: @escaping () -> Void) -> UIAction {
        UIAction(
            title: String(localized: .chatEdit),
            image: UIImage(resource: .edit).withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
        ) { _ in
            action()
        }
    }

    static func chatCopy(_ action: @escaping () -> Void) -> UIAction {
        UIAction(
            title: String(localized: .chatCopy),
            image: UIImage(resource: .copy).withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
        ) { _ in
            action()
        }
    }

    static func chatEditHistory(_ action: @escaping () -> Void) -> UIAction {
        UIAction(
            title: String(localized: .chatEditHistory),
            image: UIImage(resource: .history).withTintColor(.fgPrimary, renderingMode: .alwaysOriginal)
        ) { _ in
            action()
        }
    }
}
