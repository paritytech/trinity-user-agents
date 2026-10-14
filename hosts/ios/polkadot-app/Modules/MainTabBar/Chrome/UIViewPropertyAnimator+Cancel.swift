import UIKit

extension UIViewPropertyAnimator {
    /// Ends the animation in place. Completions still run, which the panel switch relies on.
    func cancelInPlace() {
        switch state {
        case .active:
            stopAnimation(false)
            finishAnimation(at: .current)
        case .stopped:
            finishAnimation(at: .current)
        default:
            break
        }
    }
}
