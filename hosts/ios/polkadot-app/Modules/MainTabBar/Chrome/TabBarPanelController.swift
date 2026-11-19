/// Tracks which panel is open and sequences a queued panel switch behind a close that the chrome
/// reports finished. The chrome animates the panels; this type decides which one applies.
@MainActor
final class TabBarPanelController {
    /// Receives each committed panel state; fires only when the state actually changes.
    private let onPanelChanged: (TabBarPanelKind?) -> Void
    /// Receives the open panel and triggers active action index or outside-tap state updates.
    private let onOpenPanelChanged: (TabBarPanelKind?) -> Void

    private(set) var open: TabBarPanelKind?

    private var pendingPanel: TabBarPanelKind?

    init(
        onPanelChanged: @escaping (TabBarPanelKind?) -> Void,
        onOpenPanelChanged: @escaping (TabBarPanelKind?) -> Void
    ) {
        self.onPanelChanged = onPanelChanged
        self.onOpenPanelChanged = onOpenPanelChanged
    }

    /// Sets the panel directly. Any queued switch is cancelled.
    func setPanel(_ kind: TabBarPanelKind?) {
        pendingPanel = nil
        commitPanel(kind)
    }

    /// Called by the chrome once the closing surface has finished animating. The queued panel opens here, so
    /// the reopen starts when the close ends. A fold or another `setPanel` before this call clears the queue,
    /// which cancels the switch.
    func panelCloseDidComplete() {
        guard let pendingPanel else {
            return
        }
        self.pendingPanel = nil
        setPanel(pendingPanel)
    }

    /// Selecting a different action closes the open panel before opening the new one, so the
    /// change reads as a close followed by an open instead of a silent content swap.
    func togglePanel(_ kind: TabBarPanelKind) {
        guard let open else {
            setPanel(kind)
            return
        }

        guard open != kind else {
            setPanel(nil)
            return
        }

        // Queued before the close starts, since a close can complete synchronously and open the queued panel.
        pendingPanel = kind
        commitPanel(nil)
    }
}

private extension TabBarPanelController {
    func commitPanel(_ kind: TabBarPanelKind?) {
        // `open` is cleared the moment a close starts, so a repeated close returns early.
        guard kind != nil || open != nil else {
            return
        }

        let previousPanel = open

        open = kind
        onOpenPanelChanged(kind)

        if previousPanel != kind {
            onPanelChanged(kind)
        }
    }
}
