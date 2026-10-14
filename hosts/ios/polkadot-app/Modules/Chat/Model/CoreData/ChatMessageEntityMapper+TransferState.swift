import BigInt
import CoreData
import Foundation
import Operation_iOS
import SubstrateSdk

enum TransferStateRowError: Error {
    case unknownStatus(Int16)
    case invalidActualValue(String)
}

/// Row layout shared by the message mapper (read) and `TransferStateCoreDataStore` (write).
enum TransferStateRows {
    static func state(of entity: CDChatMessage) throws -> Chat.LocalMessage.Content.Transfer.State? {
        guard let status = Chat.LocalMessage.Status(rawValue: entity.status) else {
            return nil
        }

        if status.isIncoming {
            return try entity.incomingTransferState.map { try .incoming(incoming(from: $0)) }
        } else {
            return try entity.outgoingTransferState.map { try .outgoing(outgoing(from: $0)) }
        }
    }

    static func incoming(from row: CDIncomingTransferState) throws -> IncomingTransferState {
        guard let status = IncomingTransferState.Status(rawValue: row.status) else {
            throw TransferStateRowError.unknownStatus(row.status)
        }
        return try IncomingTransferState(status: status, actualValue: balance(from: row.actualValue))
    }

    static func outgoing(from row: CDOutgoingTransferState) throws -> OutgoingTransferState {
        guard let status = OutgoingTransferState.Status(rawValue: row.status) else {
            throw TransferStateRowError.unknownStatus(row.status)
        }
        return try OutgoingTransferState(status: status, actualValue: balance(from: row.actualValue))
    }

    static func apply(_ state: IncomingTransferState, to row: CDIncomingTransferState) {
        row.status = state.status.rawValue
        row.actualValue = state.actualValue.map { String($0) }
    }

    static func apply(_ state: OutgoingTransferState, to row: CDOutgoingTransferState) {
        row.status = state.status.rawValue
        row.actualValue = state.actualValue.map { String($0) }
    }

    static func isUnchanged(_ row: CDIncomingTransferState, comparedTo state: IncomingTransferState) -> Bool {
        row.status == state.status.rawValue && row.actualValue == state.actualValue.map { String($0) }
    }

    static func isUnchanged(_ row: CDOutgoingTransferState, comparedTo state: OutgoingTransferState) -> Bool {
        row.status == state.status.rawValue && row.actualValue == state.actualValue.map { String($0) }
    }
}

private extension TransferStateRows {
    static func balance(from column: String?) throws -> Balance? {
        guard let column else { return nil }
        guard let value = BigUInt(column) else {
            throw TransferStateRowError.invalidActualValue(column)
        }
        return value
    }
}

extension Chat.LocalMessage.Content {
    func attachingTransferState(of entity: CDChatMessage) throws -> Self {
        guard case let .coinageSend(transfer) = self else { return self }
        return try .coinageSend(transfer.withState(TransferStateRows.state(of: entity)))
    }
}
