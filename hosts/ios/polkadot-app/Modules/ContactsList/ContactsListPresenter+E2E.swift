import Foundation

#if targetEnvironment(simulator)
    /// The hooks of the truapi E2E harness for the chat list (S6): a console marker with the product rooms that
    /// the list draws, and an action that draws the list again from what the app stores.
    extension ContactsListPresenter {
        func observeE2EListRender() -> NSObjectProtocol? {
            guard TrUAPIE2EMarkers.isEnabled else { return nil }

            return NotificationCenter.default.addObserver(
                forName: TrUAPIE2EHostAction.notification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let action = notification.object as? TrUAPIE2EHostAction,
                      action.action == "list-render" else { return }
                MainActor.assumeIsolated {
                    guard let self, let model = self.lastModel else { return }
                    self.provideViewModel(model)
                }
            }
        }

        /// Writes `[e2e:ui] chat-list list rooms=<name>|<name>` with the names of the product rooms that the list
        /// draws.
        func markProductRooms(_ model: ChatListModel) {
            guard TrUAPIE2EMarkers.isEnabled else { return }

            let names = model.establishedChats
                .filter { $0.chat.roomMetadata != nil }
                .map(\.peerMetadata.name)
            logger.debug("[e2e:ui] chat-list list rooms=\(names.joined(separator: "|"))")
        }
    }
#endif
