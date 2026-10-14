import Foundation

final class RestoreFromCloudWireframe: RestoreFromCloudWireframeProtocol {
    let observer: RootStateObserving

    init(observer: RootStateObserving) {
        self.observer = observer
    }
}
