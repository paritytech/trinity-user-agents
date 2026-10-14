import UIKit
import Observation
import Foundation_iOS

@MainActor @Observable
public final class ClaimUsernameViewModel {
    public var headerText: String = ""
    public var title: String = ""
    public var details: String = ""
    public var actionTitle: String = ""
    public var recoveryActionString: AttributedString?
    public var termsActionString: AttributedString?
    public var usernameInputViewModel: (any InputViewModelProtocol)?
    public var digitsOptions: [String] = []
    public var selectedDigits: String?
    public var digitsState: DigitsFieldState = .hidden
    public var usernameAvailability: UsernameAvailabilityViewModel?
    public var confirmViewState: ClaimConfirmViewState = .confirm
    public var isAccountCreationInProgress: Bool = false
    public var isUsernameInteractionEnabled: Bool = true

    public var onUsernameChanged: (() -> Void)?
    public var onDigitsChanged: (() -> Void)?
    public var onConfirm: (() -> Void)?
    public var onResolveError: (() -> Void)?
    public var onRecover: (() -> Void)?

    public init() {}
}

public enum DigitsFieldState {
    case hidden
    case loading
    case shown
}

public enum ClaimConfirmViewState {
    case loading
    case issue(String)
    case errorAction(String, UIImage?)
    case confirm
    case disabled
}
