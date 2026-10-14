import Foundation
import Individuality

extension AllowanceRecord.Kind {
    var persistenceCode: Int16 {
        switch self {
        case .pgas:
            0
        case .bulletin:
            1
        case .statementStore:
            2
        }
    }

    init?(persistenceCode: Int16) {
        switch persistenceCode {
        case 0:
            self = .pgas
        case 1:
            self = .bulletin
        case 2:
            self = .statementStore
        default:
            return nil
        }
    }
}
