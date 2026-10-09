import UIKit

@MainActor
protocol ApplicationStateProviding {
    var applicationState: UIApplication.State { get }
}

extension UIApplication: ApplicationStateProviding {}
