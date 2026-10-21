import Foundation

/// The hooks of the truapi E2E harness for the in-room search (S61, S62): a marker with what the results show, and
/// the actions of the user on the search screen.
extension ChatSearchPresenter {
    /// Writes `[e2e:ui] search <room> state=<state> hits=<count> more=<bool> ids=<id>,…` with the first five hits.
    func markState() {
        #if targetEnvironment(simulator)
            guard TrUAPIE2EMarkers.isEnabled else { return }

            let value =
                switch state {
                case .idle: "state=idle"
                case .searching: "state=searching"
                case .empty: "state=empty hits=0"
                case .unavailable: "state=unavailable"
                case let .results(rows, hasMore):
                    "state=results hits=\(rows.count) more=\(hasMore) ids="
                        + rows.prefix(5).map { productMessageId($0.messageId) }.joined(separator: ",")
                }
            logger.debug("[e2e:ui] search \(roomId) \(value)")
        #endif
    }

    /// The id under which the product knows a message, as the markers of the room screen name it.
    private func productMessageId(_ messageId: Chat.MessageId) -> String {
        messageId.components(separatedBy: "/").last ?? messageId
    }
}

#if targetEnvironment(simulator)
    extension ChatSearchPresenter {
        func observeE2EActions() -> NSObjectProtocol? {
            guard TrUAPIE2EMarkers.isEnabled else { return nil }

            return NotificationCenter.default.addObserver(
                forName: TrUAPIE2EHostAction.notification,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let action = notification.object as? TrUAPIE2EHostAction else { return }
                MainActor.assumeIsolated { self?.runE2E(action) }
            }
        }

        private func runE2E(_ action: TrUAPIE2EHostAction) {
            guard action.room == roomId, view?.controller.view.window != nil else { return }

            switch action.action {
            case "search":
                (view as? ChatSearchViewController)?.setSearchText(action.argument ?? "")
                search(text: action.argument ?? "")
            case "search-more":
                loadMore()
            case "search-select":
                let target = action.target ?? "first"
                let hit = target == "last" ? hits.last
                    : target == "first" ? hits.first
                    : hits.first { productMessageId($0.messageId) == target }
                if let hit {
                    select(messageId: hit.messageId)
                }
            default:
                return
            }
        }
    }
#endif
