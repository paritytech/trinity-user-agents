import UIKit
import PolkadotUI
import Individuality

@MainActor
final class PolkadotSignInPresenter {
    weak var view: PolkadotSignInViewProtocol?

    let interactor: PolkadotSignInInteractorInputProtocol
    let wireframe: PolkadotSignInWireframeProtocol

    private enum State {
        case loading
        case loaded(HandshakeInput)
        case sendingHandshake
    }

    private var state: State = .loading

    init(
        interactor: PolkadotSignInInteractorInputProtocol,
        wireframe: PolkadotSignInWireframeProtocol
    ) {
        self.interactor = interactor
        self.wireframe = wireframe
    }
}

extension PolkadotSignInPresenter: PolkadotSignInPresenterProtocol {
    func setup() {
        interactor.setup()
        provideViewModel()
    }

    func approve() {
        guard case let .loaded(input) = state else {
            return
        }
        interactor.approve(with: input)
    }

    func cancel() {
        wireframe.hide(view: view, with: nil)
    }
}

extension PolkadotSignInPresenter: PolkadotSignInInteractorOutputProtocol {
    func didStartFetchingInput() {
        state = .loading
        provideViewModel()
    }

    func didFinishFetchingInput(_ input: HandshakeInput) {
        state = .loaded(input)
        provideViewModel()
    }

    func didFailToFetchInput(with _: Error) {
        wireframe.hide(view: view, with: .failed)
    }

    func didStartSendingHandshake() {
        state = .sendingHandshake
        provideViewModel()
    }

    func didFinishSendingHandshake(with device: Chat.LocalDevice) {
        wireframe.hide(view: view, with: .success(device))
    }

    func didFailToSendHandshake(with error: Error) {
        if let message = noSlotsMessage(for: error) {
            wireframe.hide(view: view, with: .noFreeSlots(message: message))
            return
        }

        wireframe.hide(view: view, with: .failed)
    }
}

private extension PolkadotSignInPresenter {
    func noSlotsMessage(for error: Error) -> String? {
        if case AllowanceSlotAssignmentError.noSlotsAvailable = error {
            return String(localized: .polkadotSignInNoSlots)
        }

        guard let allowanceError = error as? StatementStoreAllowanceError else {
            return nil
        }

        switch allowanceError {
        case .noEvictableSlots:
            return String(localized: .polkadotSignInNoSlots)
        case let .noSlotsAvailable(secsToWait):
            let timeString = secsToWait.localizedDaysHoursOrFallbackMinutes()
            return String(localized: .polkadotSignInNoSlotsAvailable(time: timeString))
        }
    }

    func provideViewModel() {
        switch state {
        case .loading:
            view?.didReceive(viewModel: .inProgress)
        case let .loaded(input):
            view?.didReceive(viewModel: .result(.init(
                deviceDescription: makeDeviceDescription(from: input)
            )))
        case .sendingHandshake:
            view?.didReceive(viewModel: .sendingHandshake)
        }
    }

    func makeDeviceDescription(from input: HandshakeInput) -> String {
        let deviceData = input.hostData.deviceData

        let hostName =
            if let version = deviceData.hostVersion {
                "\(input.metadata.name) v.\(version)"
            } else {
                input.metadata.name
            }

        let deviceParts = [
            deviceData.platformType,
            deviceData.platformVersion
        ].compactMap { $0 }

        if deviceParts.isEmpty {
            return hostName
        }

        let deviceName = deviceParts.joined(separator: " ")
        return "\(hostName)\n(\(deviceName))"
    }
}
