import Foundation
import UIKit
import UIKitExt

/// The mute and the archive entries of the menu of the room header (P10.10, 07-LATER §6.2).
extension ChatProductRoomPresenter {
    /// The entries of the header menu. The titles follow the state when the menu opens.
    func headerMenuActions() -> [Chat.PeerAction] {
        userStateActions() + peerMenuActions()
    }

    private func userStateActions() -> [Chat.PeerAction] {
        guard userStates != nil else { return [] }

        let mute = Chat.CustomPeerAction(
            titleProvider: { [weak self] in ChatMuteOptions.muteTitle(isMuted: self?.isMuted ?? false) },
            image: ChatMuteOptions.muteImage,
            handler: { [weak self] in self?.toggleMute() }
        )
        let archive = Chat.CustomPeerAction(
            titleProvider: { [weak self] in ChatMuteOptions.archiveTitle(isArchived: self?.isArchived ?? false) },
            image: ChatMuteOptions.archiveImage(isArchived: false),
            handler: { [weak self] in self?.applyUserState(.archive(!(self?.isArchived ?? false))) }
        )
        return [.custom(mute), .custom(archive)]
    }

    var isMuted: Bool {
        policy?.userState.isMuted(at: Date()) ?? false
    }

    var isArchived: Bool {
        policy?.userState.archived ?? false
    }

    /// Unmutes a muted room. For a room that is not muted, asks how long to mute it.
    func toggleMute() {
        guard !isMuted else {
            applyUserState(.unmute)
            return
        }

        wireframe.showMuteOptions(from: view) { [weak self] duration in
            self?.applyUserState(.mute(duration))
        }
    }

    func applyUserState(_ change: ChatRoomUserStateChange) {
        guard let userStates else { return }

        Task { [chatId, logger] in
            do {
                try await userStates.apply(change, to: chatId)
            } catch {
                logger.error("The user state of \(chatId.rawRepresentation) did not change: \(error)")
            }
        }
    }
}

extension ChatProductRoomWireframing {
    /// Asks how long to mute the room.
    func showMuteOptions(from view: ControllerBackedProtocol?, onSelect: @escaping (Chat.MuteDuration) -> Void) {
        view?.controller.present(ChatMuteOptions.sheet(onSelect: onSelect), animated: true)
    }
}
