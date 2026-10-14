import Foundation

final class DataValidationRunner {
    let validators: [DataValidating]

    private var lastIndex: Int = 0
    private var completionClosure: DataValidationRunnerCompletion?
    private var resumeClosure: DataValidationRunnerResumeClosure?
    private var stopClosure: DataValidationRunnerStopClosure?

    init(validators: [DataValidating]) {
        self.validators = validators
    }

    deinit {
        lastIndex = 0
    }

    private func runValidation(from startIndex: Int) {
        resumeClosure?(startIndex)

        for index in startIndex ..< validators.count {
            guard let problem = validators[index].validate(notifying: self) else {
                continue
            }
            stopClosure?(problem)

            switch problem {
            case .warning:
                lastIndex = index
                return
            case .asyncProcess:
                lastIndex = index
                return
            case .error:
                return
            }
        }

        completionClosure?()
    }
}

extension DataValidationRunner: DataValidationRunnerProtocol {
    func runValidation(
        notifyingOnSuccess completionClosure: @escaping DataValidationRunnerCompletion,
        notifyingOnStop stopClosure: DataValidationRunnerStopClosure?,
        notifyingOnResume resumeClosure: DataValidationRunnerResumeClosure?
    ) {
        self.completionClosure = completionClosure
        self.stopClosure = stopClosure
        self.resumeClosure = resumeClosure

        runValidation(from: 0)
    }
}

extension DataValidationRunner: DataValidatingDelegate {
    func didCompleteWarningHandling() {
        runValidation(from: lastIndex + 1)
    }

    func didCompleteAsyncHandling() {
        runValidation(from: lastIndex + 1)
    }
}
