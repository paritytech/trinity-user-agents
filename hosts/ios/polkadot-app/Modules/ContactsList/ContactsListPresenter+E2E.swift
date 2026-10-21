import Foundation

#if targetEnvironment(simulator)
    /// The hooks of the truapi E2E harness for the chat list (S6, S54, S55, S63): console markers with the product
    /// rooms that the list draws and their user state, and the actions of the user on the list.
    extension ContactsListPresenter {
        func observeE2EListRender() -> NSObjectProtocol? {
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

        /// Writes `[e2e:ui] chat-list list rooms=<name>|<name>` (`chat-archive` for the archived list) with the names
        /// of the product rooms that the list draws, `[e2e:ui] archived-row list count=<count>` for the main list,
        /// and `[e2e:ui] row <room> <scope> muted=<bool> archived=<bool>` for each product room of the list.
        func markProductRooms(_ model: ChatListModel) {
            guard TrUAPIE2EMarkers.isEnabled else { return }

            let rooms = model.establishedChats.filter { $0.chat.roomMetadata != nil }
            let names = rooms.map(\.peerMetadata.name)
            let key = scope == .main ? "chat-list" : "chat-archive"
            logger.debug("[e2e:ui] \(key) list rooms=\(names.joined(separator: "|"))")
            if scope == .main {
                logger.debug("[e2e:ui] archived-row list count=\(model.archivedChatCount)")
            }

            for room in rooms {
                guard let roomId = room.chat.chatId.roomId else { continue }
                let state = room.chat.roomDeclarations?.userState ?? .default
                logger.debug(
                    "[e2e:ui] row \(roomId) \(scope == .main ? "main" : "archived")"
                        + " muted=\(state.isMuted(at: Date())) archived=\(state.archived)"
                )
            }
        }
    }

    extension ContactsListPresenter {
        /// Writes `[e2e:ui] list-search list query=<text> rooms=<name>|<name> hits=<room>:<message>|…`.
        func markSearchResults() {
            guard TrUAPIE2EMarkers.isEnabled, let searchResults else { return }

            let rooms = searchResults.chats.map(\.peerMetadata.name).joined(separator: "|")
            let hits = searchResults.hits.map { hit in
                let room = hit.chat.chat.chatId.roomId ?? "person"
                return "\(room):\(hit.hit.messageId.components(separatedBy: "/").last ?? hit.hit.messageId)"
            }
            logger.debug(
                "[e2e:ui] list-search list query=\(searchResults.query) rooms=\(rooms) hits=\(hits.joined(separator: "|"))"
            )
        }
    }

    @MainActor
    private extension ContactsListPresenter {
        // swiftlint:disable:next cyclomatic_complexity
        func runE2E(_ action: TrUAPIE2EHostAction) {
            switch action.action {
            case "list-search":
                guard scope == .main else { return }
                (view as? ContactsListViewController)?.setSearchText(action.argument ?? "")
                search(text: action.argument ?? "")
            case "list-render":
                guard let lastModel else { return }
                provideViewModel(lastModel)
            case "archived":
                guard scope == .main else { return }
                showArchivedChats()
            case "list-mute":
                applyE2E(.mute(Chat.MuteDuration(e2eName: action.argument)), room: action.room)
            case "list-unmute":
                applyE2E(.unmute, room: action.room)
            case "list-archive":
                applyE2E(.archive(true), room: action.room)
            case "list-unarchive":
                applyE2E(.archive(false), room: action.room)
            default:
                return
            }
        }

        /// Runs a list action on the room as a swipe or a menu entry runs it. Only the list that shows the room acts.
        func applyE2E(_ change: ChatRoomUserStateChange, room: String) {
            guard let identifier = lastModel?.establishedChats
                .first(where: { $0.chat.chatId.roomId == room })?
                .chat.identifier,
                let actions = rowActions(for: identifier)
            else {
                return
            }

            logger.debug("[e2e:ui] row-actions \(room) mute=\(!actions.isMuted) archive=\(!actions.isArchived)")
            apply(change, contactIdentifier: identifier)
        }
    }

    extension Chat.MuteDuration {
        /// `1h`, `8h`, `1w` or `always`, as the harness names a duration.
        init(e2eName: String?) {
            switch e2eName {
            case "8h": self = .eightHours
            case "1w": self = .oneWeek
            case "always": self = .always
            default: self = .oneHour
            }
        }
    }
#endif
