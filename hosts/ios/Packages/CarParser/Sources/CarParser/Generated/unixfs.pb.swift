// DO NOT EDIT.
// swift-protobuf https://github.com/apple/swift-protobuf
//
// Generated from: unixfs.proto

import Foundation
import SwiftProtobuf

// MARK: - UnixFsData

/// UnixFS protobuf data message.
/// Note: named `UnixFsData` to avoid collision with Foundation.Data.
struct UnixFsData {
    var type: DataType = .raw
    var data: Data?
    var filesize: UInt64?
    var blocksizes: [UInt64] = []
    var hashType: UInt64?
    var fanout: UInt64?

    var unknownFields = SwiftProtobuf.UnknownStorage()

    enum DataType: SwiftProtobuf.Enum {
        typealias RawValue = Int

        case raw // = 0
        case directory // = 1
        case file // = 2
        case metadata // = 3
        case symlink // = 4
        case hamtShard // = 5

        init() { self = .raw }

        init?(rawValue: Int) {
            switch rawValue {
            case 0: self = .raw
            case 1: self = .directory
            case 2: self = .file
            case 3: self = .metadata
            case 4: self = .symlink
            case 5: self = .hamtShard
            default: return nil
            }
        }

        var rawValue: Int {
            switch self {
            case .raw: 0
            case .directory: 1
            case .file: 2
            case .metadata: 3
            case .symlink: 4
            case .hamtShard: 5
            }
        }
    }

    init() {}
}

extension UnixFsData: SwiftProtobuf.Message, SwiftProtobuf._MessageImplementationBase,
    SwiftProtobuf._ProtoNameProviding {
    static let protoMessageName: String = "unixfs.Data"
    static let _protobuf_nameMap: SwiftProtobuf._NameMap = [
        1: .same(proto: "Type"),
        2: .same(proto: "Data"),
        3: .same(proto: "filesize"),
        4: .same(proto: "blocksizes"),
        5: .same(proto: "hashType"),
        6: .same(proto: "fanout"),
    ]

    var hasData: Bool { data != nil }
    var hasFilesize: Bool { filesize != nil }
    var hasHashType: Bool { hashType != nil }
    var hasFanout: Bool { fanout != nil }

    mutating func decodeMessage(decoder: inout some SwiftProtobuf.Decoder) throws {
        while let fieldNumber = try decoder.nextFieldNumber() {
            switch fieldNumber {
            case 1: try decoder.decodeSingularEnumField(value: &type)
            case 2: try decoder.decodeSingularBytesField(value: &data)
            case 3: try decoder.decodeSingularUInt64Field(value: &filesize)
            case 4: try decoder.decodeRepeatedUInt64Field(value: &blocksizes)
            case 5: try decoder.decodeSingularUInt64Field(value: &hashType)
            case 6: try decoder.decodeSingularUInt64Field(value: &fanout)
            default: break
            }
        }
    }

    func traverse(visitor: inout some SwiftProtobuf.Visitor) throws {
        if type != .raw {
            try visitor.visitSingularEnumField(value: type, fieldNumber: 1)
        }
        try { if let v = self.data {
            try visitor.visitSingularBytesField(value: v, fieldNumber: 2)
        } }()
        try { if let v = self.filesize {
            try visitor.visitSingularUInt64Field(value: v, fieldNumber: 3)
        } }()
        if !blocksizes.isEmpty {
            try visitor.visitRepeatedUInt64Field(value: blocksizes, fieldNumber: 4)
        }
        try { if let v = self.hashType {
            try visitor.visitSingularUInt64Field(value: v, fieldNumber: 5)
        } }()
        try { if let v = self.fanout {
            try visitor.visitSingularUInt64Field(value: v, fieldNumber: 6)
        } }()
        try unknownFields.traverse(visitor: &visitor)
    }

    static func == (lhs: UnixFsData, rhs: UnixFsData) -> Bool {
        if lhs.type != rhs.type { return false }
        if lhs.data != rhs.data { return false }
        if lhs.filesize != rhs.filesize { return false }
        if lhs.blocksizes != rhs.blocksizes { return false }
        if lhs.hashType != rhs.hashType { return false }
        if lhs.fanout != rhs.fanout { return false }
        if lhs.unknownFields != rhs.unknownFields { return false }
        return true
    }
}

extension UnixFsData.DataType: CaseIterable {
    static let allCases: [UnixFsData.DataType] = [
        .raw, .directory, .file, .metadata, .symlink, .hamtShard,
    ]
}

extension UnixFsData.DataType: SwiftProtobuf._ProtoNameProviding {
    static let _protobuf_nameMap: SwiftProtobuf._NameMap = [
        0: .same(proto: "Raw"),
        1: .same(proto: "Directory"),
        2: .same(proto: "File"),
        3: .same(proto: "Metadata"),
        4: .same(proto: "Symlink"),
        5: .same(proto: "HAMTShard"),
    ]
}
