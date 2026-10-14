import Foundation

public protocol JSEngineScriptHandling {
    func getScript() -> JSEngineScript

    var handlerName: String { get }

    func handle(body: Any)
}
