import FoundationExt
import UIKit

protocol ChatSigningConsentPresenting: Sendable {
    func askConsent(for subject: ChatSigningConsentSubject) async -> Bool
}

/// Shows the consent alert over the top view controller once the app is active.
final class ChatSigningConsentAlertPresenter: ChatSigningConsentPresenting {
    private let appStateStreamFactory: ApplicationStateStreamFactory

    init(appStateStreamFactory: ApplicationStateStreamFactory = ApplicationStateStreamFactory()) {
        self.appStateStreamFactory = appStateStreamFactory
    }

    func askConsent(for subject: ChatSigningConsentSubject) async -> Bool {
        await waitUntilActive()
        return await presentAlert(for: subject)
    }
}

private extension ChatSigningConsentAlertPresenter {
    func waitUntilActive() async {
        let activations = appStateStreamFactory.stream(for: .didBecomeActive)

        guard await UIApplication.shared.applicationState != .active else { return }

        for await _ in activations {
            return
        }
    }

    @MainActor
    func presentAlert(for subject: ChatSigningConsentSubject) async -> Bool {
        guard let presenter = UIWindow.topWindow?.rootViewController?.topModalViewController else {
            return false
        }

        return await withCheckedContinuation { continuation in
            let alert = UIAlertController(
                title: String(localized: .chatSigningConsentTitle),
                message: message(for: subject),
                preferredStyle: .alert
            )

            alert.addAction(UIAlertAction(title: String(localized: .chatSigningConsentDecline), style: .cancel) { _ in
                continuation.resume(returning: false)
            })

            alert.addAction(UIAlertAction(title: String(localized: .chatSigningConsentAccept), style: .default) { _ in
                continuation.resume(returning: true)
            })

            presenter.present(alert, animated: true)
        }
    }

    func message(for subject: ChatSigningConsentSubject) -> String {
        guard let username = subject.username else {
            return String(localized: .chatSigningConsentMessage)
        }

        return String(localized: .chatSigningConsentMessageWithUsername(username: username))
    }
}
