import DesignSystem
import PolkadotUI
import SwiftUI

@MainActor
@Observable
final class ThemeSelectionViewModel {
    enum Context {
        case onboarding
        case settings
    }

    struct Swatch: Identifiable {
        let id: ThemeSelection
        let index: Int
        let name: String
        let backgroundColor: Color
        let foregroundColor: Color
    }

    struct PreviewMessage: Identifiable {
        let id: Int
        let text: String
        let sender: DSChatMessageBubble.Sender
        let timestamp: String
        let deliveryStatus: DSChatMessageBubble.DeliveryStatus?
        let reference: DSChatMessageBubble.Reference?
    }

    let swatches: [Swatch]
    let previewMessages: [PreviewMessage]
    private(set) var selected: ThemeSelection
    private(set) var committed: ThemeSelection

    var title: String { String(localized: .themeSelectionTitle) }
    var subtitle: String { String(localized: .themeSelectionSubtitle) }

    var confirmTitle: String { String(localized: .themeSelectionContinue) }

    var showsBackButton: Bool { context == .settings }

    var confirmsOnButton: Bool { context == .onboarding }

    var showsConfirmButton: Bool { confirmsOnButton }

    var waitsForBubblesBeforeChrome: Bool { context == .onboarding }

    private let context: Context
    private let themeManager: ThemeManagerProtocol
    private let storage: ThemeSelectionStoring
    private let onComplete: () -> Void

    init(
        themeManager: ThemeManagerProtocol,
        storage: ThemeSelectionStoring = ThemeSelectionStorage(),
        context: Context = .onboarding,
        onComplete: @escaping () -> Void
    ) {
        self.context = context
        self.themeManager = themeManager
        self.storage = storage
        self.onComplete = onComplete

        let current = Self.selection(from: themeManager.mode)
        selected = current
        committed = current
        previewMessages = Self.makePreviewMessages()

        // Each swatch shows its own theme's colors
        swatches = ThemeSelection.allCases.enumerated().map { index, selection in
            let colors = ThemesRegistry.makeTheme(selection).colors
            return Swatch(
                id: selection,
                index: index,
                name: selection.displayName,
                backgroundColor: Color(uiColor: colors.bgActionSecondary),
                foregroundColor: Color(uiColor: colors.bgSurfaceContainerInverted)
            )
        }
    }

    func select(_ selection: ThemeSelection) {
        guard selection != selected else { return }

        selected = selection

        guard context == .settings else { return }

        applyTheme(selection)
        storage.setSelected()
    }

    func commit(_ selection: ThemeSelection) {
        committed = selection
    }

    func applyTheme(_ selection: ThemeSelection) {
        themeManager.select(.app(selection))
    }

    func confirm() {
        applyTheme(selected)
        storage.setSelected()
        onComplete()
    }

    func back() {
        onComplete()
    }
}

private extension ThemeSelectionViewModel {
    static func selection(from mode: ThemeMode) -> ThemeSelection {
        switch mode {
        case let .app(selection): selection
        }
    }

    static func makePreviewMessages() -> [PreviewMessage] {
        [
            PreviewMessage(
                id: 0,
                text: String(localized: .themeSelectionPreviewMessage1),
                sender: .me,
                timestamp: "12:22",
                deliveryStatus: .delivered,
                reference: nil
            ),
            PreviewMessage(
                id: 1,
                text: String(localized: .themeSelectionPreviewMessage2),
                sender: .other,
                timestamp: "12:40",
                deliveryStatus: nil,
                reference: .init(
                    senderName: String(localized: .themeSelectionPreviewReplySender),
                    text: String(localized: .themeSelectionPreviewMessage1)
                )
            ),
            PreviewMessage(
                id: 2,
                text: String(localized: .themeSelectionPreviewMessage3),
                sender: .me,
                timestamp: "12:41",
                deliveryStatus: .sent,
                reference: nil
            )
        ]
    }
}
