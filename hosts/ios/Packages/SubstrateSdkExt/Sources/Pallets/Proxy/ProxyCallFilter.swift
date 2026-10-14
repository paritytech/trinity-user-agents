import Foundation
import SubstrateSdk

protocol ProxyCallFilterProtocol {
    func matches(call: CallCodingPath) -> Bool
}

enum ProxyCallFilter {
    struct ConstantMatches: ProxyCallFilterProtocol {
        let result: Bool

        init(result: Bool) {
            self.result = result
        }

        func matches(call _: CallCodingPath) -> Bool {
            result
        }
    }

    struct MatchesPallet: ProxyCallFilterProtocol {
        let palletPossibleNames: Set<String>

        init(pallet: String) {
            palletPossibleNames = [pallet]
        }

        init(palletPossibleNames: Set<String>) {
            self.palletPossibleNames = palletPossibleNames
        }

        func matches(call: CallCodingPath) -> Bool {
            palletPossibleNames.contains(call.moduleName)
        }
    }

    struct MatchesCall: ProxyCallFilterProtocol {
        let callPath: CallCodingPath

        init(callPath: CallCodingPath) {
            self.callPath = callPath
        }

        func matches(call: CallCodingPath) -> Bool {
            callPath == call
        }
    }

    struct NotMatches: ProxyCallFilterProtocol {
        let innerFilter: ProxyCallFilterProtocol

        init(innerFilter: ProxyCallFilterProtocol) {
            self.innerFilter = innerFilter
        }

        func matches(call: CallCodingPath) -> Bool {
            !innerFilter.matches(call: call)
        }
    }

    struct OrMatches: ProxyCallFilterProtocol {
        let innerFilters: [ProxyCallFilterProtocol]

        init(innerFilters: [ProxyCallFilterProtocol]) {
            self.innerFilters = innerFilters
        }

        func matches(call: CallCodingPath) -> Bool {
            innerFilters.contains { $0.matches(call: call) }
        }
    }
}
