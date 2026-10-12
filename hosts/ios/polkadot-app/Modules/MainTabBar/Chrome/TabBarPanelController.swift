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

    func setPanel(_ kind: TabBarPanelKind?) {
        pendingPanel = nil

        // `open` is cleared the moment a close starts, so a repeated close returns early. The pending
        // reopen above is still cleared, which is all a repeated close can mean.
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

    /// Called by the chrome once the closing surface has finished animating. The queued panel opens here, so
    /// the reopen starts when the close ends. `togglePanel` sets `pendingPanel` after its close returns, and a
    /// fold or another tap before this call clears it, which cancels the switch.
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

        setPanel(nil)
        pendingPanel = kind
    }
}
