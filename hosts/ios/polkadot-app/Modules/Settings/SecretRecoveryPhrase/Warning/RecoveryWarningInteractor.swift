import UIKit
import Combine

final class RecoveryWarningInteractor {
    weak var presenter: RecoveryWarningInteractorOutputProtocol?
}

extension RecoveryWarningInteractor: RecoveryWarningInteractorInputProtocol {}
