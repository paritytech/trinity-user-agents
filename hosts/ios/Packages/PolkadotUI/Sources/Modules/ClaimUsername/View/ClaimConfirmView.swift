import ExternalAccessibility
import SwiftUI

struct ClaimConfirmView: View {
    let state: ClaimConfirmViewState
    let actionTitle: String
    var onAction: (() -> Void)?
    var onError: (() -> Void)?

    var body: some View {
        Group {
            switch state {
            case .confirm:
                actionButton(enabled: true)
            case .disabled:
                actionButton(enabled: false)
            case .loading:
                loadingButton
            case let .issue(title):
                issueLabel(title)
            case let .errorAction(title, _):
                errorButton(title: title)
            }
        }
        .frame(height: UIConstants.actionHeight)
    }

    private func actionButton(enabled: Bool) -> some View {
        DSButton(actionTitle, style: .primary, expands: true) {
            onAction?()
        }
        .disabled(!enabled)
        .accessibilityId(AccessibilityID.Username.submitButton)
    }

    private var loadingButton: some View {
        ZStack {
            RoundedRectangle(cornerRadius: UIConstants.actionHeight / 2)
                .fill(Color(.fgPrimary))
            ProgressView()
                .tint(Color(.fgPrimaryInverted))
        }
    }

    private func issueLabel(_ title: String) -> some View {
        Text(title.uppercased())
            .typography(.titleMedium)
            .frame(maxWidth: .infinity)
            .frame(height: UIConstants.actionHeight)
            .background(Color(.bgActionDisabled), in: RoundedRectangle(cornerRadius: UIConstants.actionHeight / 2))
            .foregroundStyle(Color(.fgDisabled))
    }

    private func errorButton(title: String) -> some View {
        DSButton(title, style: .primary, expands: true) {
            onError?()
        }
        .accessibilityId(AccessibilityID.Username.submitButton)
    }
}
