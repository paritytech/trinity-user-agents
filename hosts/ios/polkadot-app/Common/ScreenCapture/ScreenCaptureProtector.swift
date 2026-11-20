import UIKit
import UIKitExt
import FoundationExt
import AsyncExtensions

/// Hides sensitive views from screen capture and reports when the screen content is exposed:
/// while the scene is captured (recording, mirroring) or the app is inactive (app switcher snapshot).
@MainActor
final class ScreenCaptureProtector {
    private let appStateStreamFactory: ApplicationStateStreamFactory

    private weak var viewController: UIViewController?
    private var onExposureChange: ((Bool) -> Void)?
    private var isResignedActive = false
    private var appStateTask: Task<Void, Never>?

    var isExposed: Bool {
        isResignedActive || viewController?.traitCollection.sceneCaptureState == .active
    }

    init(appStateStreamFactory: ApplicationStateStreamFactory) {
        self.appStateStreamFactory = appStateStreamFactory
    }

    deinit {
        appStateTask?.cancel()
    }

    func protect(
        _ views: [UIView],
        in viewController: UIViewController,
        onExposureChange: @escaping (Bool) -> Void
    ) {
        views.forEach { $0.layer.setScreenCaptureDisabled(true) }

        self.viewController = viewController
        self.onExposureChange = onExposureChange

        viewController.registerForTraitChanges(
            [UITraitSceneCaptureState.self]
        ) { [weak self] (_: UIViewController, _: UITraitCollection) in
            self?.notifyExposure()
        }

        appStateTask?.cancel()
        appStateTask = observeApplicationActivity()
    }
}

// MARK: - Private

private extension ScreenCaptureProtector {
    func observeApplicationActivity() -> Task<Void, Never> {
        let resignEvents = appStateStreamFactory.stream(for: .willResignActive).map { true }
        let activeEvents = appStateStreamFactory.stream(for: .didBecomeActive).map { false }

        return Task { [weak self] in
            for await isResignedActive in merge(resignEvents, activeEvents) {
                self?.isResignedActive = isResignedActive
                self?.notifyExposure()
            }
        }
    }

    func notifyExposure() {
        onExposureChange?(isExposed)
    }
}
