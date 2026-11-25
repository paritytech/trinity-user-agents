import Foundation
import PolkadotUI
import SubstrateSdk
import UIKit.UIImage

extension ChatMetadata {
    func chatContactInfo(icon: AnyImageViewModel?) -> ChatHeaderConfiguration {
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
            icon: icon,
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

    func outgoingViewState(reclaimedAmount: String?) -> ChatTransferMessageConfiguration.OutgoingState {
        guard case let .outgoing(outgoing) = state else { return .sending }
        return outgoing.status.viewState(reclaimedAmount: reclaimedAmount)
    }

    /// Only an unclaimed or failed reclaim can be retried by the sender.
    var isReclaimable: Bool {
        guard case let .outgoing(outgoing) = state else { return false }
        return outgoing.status == .unclaimed || outgoing.status == .reclaimFailed
    }

    /// The reclaimed amount when it falls short of the message total; `nil` when the whole payment came back.
    var partialReclaimedValue: Balance? {
        guard
            case let .outgoing(outgoing) = state,
            outgoing.status.isReclaimed,
            let actualValue = outgoing.actualValue,
            actualValue != totalValue
        else {
            return nil
        }
        return actualValue
    }
}

private extension IncomingTransferState.Status {
    var viewState: ChatTransferMessageConfiguration.IncomingState {
        switch self {
        case .detecting: .detecting
        case .claiming: .claiming
        case .claimed,
             .claimedFinalized: .claimed
        case .failed: .failed
        case .returnedToSender: .returnedToSender
        }
    }
}

private extension OutgoingTransferState.Status {
    func viewState(reclaimedAmount: String?) -> ChatTransferMessageConfiguration.OutgoingState {
        switch self {
        case .sending: .sending
        case .sent,
             .sentFinalized: .sent
        case .unclaimed: .unclaimed
        case .claimed,
             .claimedFinalized: .claimed
        case .failed: .failed
        case .reclaiming: .reclaiming
        case .reclaimed,
             .reclaimedFinalized: .reclaimed(partialAmount: reclaimedAmount)
        case .reclaimFailed: .reclaimFailed
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

    static func chatDelete(_ action: @escaping () -> Void) -> UIAction {
        UIAction(
            title: String(localized: .chatDelete),
            image: UIImage(systemName: "trash"),
            attributes: .destructive
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
