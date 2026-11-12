import UIKit

@MainActor
final class RootPresenter {
    private static let loadingHintDelay: Duration = .seconds(3)

    weak var view: RootViewProtocol?
    let wireframe: RootWireframeProtocol
    let interactor: RootInteractorInputProtocol
    let viewModelFactory: RootViewModelMaking

    private var onComplete: (() -> Void)?
    private var loadingHintTask: Task<Void, Never>?

    init(
        wireframe: RootWireframeProtocol,
        interactor: RootInteractorInputProtocol,
        viewModelFactory: RootViewModelMaking
    ) {
        self.wireframe = wireframe
        self.interactor = interactor
        self.viewModelFactory = viewModelFactory
    }
}

extension RootPresenter: RootPresenterProtocol {
    func loadOnLaunch(onComplete: @escaping () -> Void) {
        self.onComplete = onComplete

        view?.didReceive(viewModel: viewModelFactory.makeInitial())
        scheduleLoadingHint()
        interactor.setup()
    }

    func retry() {
        view?.didReceive(viewModel: viewModelFactory.makeInitial())
        scheduleLoadingHint()
        interactor.retrySetup()
    }
}

extension RootPresenter: RootInteractorOutputProtocol {
    func didDecide(destination: RootDestination) {
        cancelLoadingHint()
        show(destination)

        onComplete?()
        onComplete = nil
    }

    func didFailSetup(kind: RootSetupFailureKind) {
        cancelLoadingHint()
        view?.didReceive(viewModel: viewModelFactory.makeFailure(kind: kind))
    }

    func didRecoverConnectivity() {
        retry()
    }

    #if TESTNET_FEATURE
        func didRequireAppFactoryReset() {
            wireframe.showAppFactoryResetSheet()
        }
    #endif
}

private extension RootPresenter {
    func show(_ destination: RootDestination) {
        switch destination {
        case .selectTheme:
            wireframe.showThemeSelection(with: self)
        case .onboarding:
            wireframe.showOnboarding(with: self)
        case .restoreFromCloud:
            wireframe.showRestoreFromCloud(with: self)
        case .usernameCheck:
            wireframe.showUsernameCheck(with: self)
        case .dashboard:
            wireframe.showDashboard()
        case .broken:
            wireframe.showBroken()
        }
    }

    func scheduleLoadingHint() {
        cancelLoadingHint()

        loadingHintTask = Task { [weak self] in
            try? await Task.sleep(for: Self.loadingHintDelay)
            guard !Task.isCancelled, let self else { return }

            view?.didReceive(viewModel: viewModelFactory.makeLoadingHint())
        }
    }

    func cancelLoadingHint() {
        loadingHintTask?.cancel()
        loadingHintTask = nil
    }
}

extension RootPresenter: RootStateObserving {
    func didCreateWallets() {
        interactor.completeWalletsCreation()
    }

    func didRestoreWallets() {
        interactor.completeWalletsRecovery()
    }

    func didDecideBroken() {
        show(.broken)
    }

    func didClaimUsername() {
        interactor.reevaluate()
    }

    func didDecideClaim() {
        wireframe.showUsernameClaim(with: self)
    }

    func didSelectTheme() {
        interactor.reevaluate()
    }
}
