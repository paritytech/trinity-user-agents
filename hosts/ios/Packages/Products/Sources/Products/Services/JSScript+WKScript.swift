import Foundation
import WebKit

public extension JSEngineScript.InsertionPoint {
    var toWkInjectionTime: WKUserScriptInjectionTime {
        switch self {
        case .atDocStart:
            .atDocumentStart
        case .atDocEnd:
            .atDocumentEnd
        }
    }
}
