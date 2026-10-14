import TipKit

/// Answers which tip should be on screen and signals when that answer may have changed.
@MainActor
protocol TabBarTipSequenceProtocol: AnyObject {
    var currentStep: TabBarTipStep? { get }

    /// Runs until cancelled, invoking `onChange` whenever `currentStep` may have changed.
    func observeChanges(_ onChange: @escaping @MainActor () -> Void) async
}
