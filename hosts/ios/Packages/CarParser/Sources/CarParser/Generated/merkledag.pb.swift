// DO NOT EDIT.
// swift-protobuf https://github.com/apple/swift-protobuf
//
// Generated from: merkledag.proto

import Foundation
import SwiftProtobuf

// MARK: - PBLink

struct PBLink {
    var hash: Data?
    var name: String?
    var tsize: UInt64?

    var unknownFields = SwiftProtobuf.UnknownStorage()

    init() {}
}

extension PBLink: SwiftProtobuf.Message, SwiftProtobuf._MessageImplementationBase, SwiftProtobuf._ProtoNameProviding {
    static let protoMessageName: String = "merkledag.PBLink"
    static let _protobuf_nameMap: SwiftProtobuf._NameMap = [
        1: .same(proto: "Hash"),
        2: .same(proto: "Name"),
        3: .same(proto: "Tsize"),
    ]

    var hasHash: Bool { hash != nil }
    var hasName: Bool { name != nil }
    var hasTsize: Bool { tsize != nil }

    mutating func decodeMessage(decoder: inout some SwiftProtobuf.Decoder) throws {
        while let fieldNumber = try decoder.nextFieldNumber() {
            switch fieldNumber {
            case 1: try decoder.decodeSingularBytesField(value: &hash)
            case 2: try decoder.decodeSingularStringField(value: &name)
            case 3: try decoder.decodeSingularUInt64Field(value: &tsize)
            default: break
            }
        }
    }

    func traverse(visitor: inout some SwiftProtobuf.Visitor) throws {
        if let v = hash {
            try visitor.visitSingularBytesField(value: v, fieldNumber: 1)
        }
        try { if let v = self.name {
            try visitor.visitSingularStringField(value: v, fieldNumber: 2)
        } }()
        try { if let v = self.tsize {
            try visitor.visitSingularUInt64Field(value: v, fieldNumber: 3)
        } }()
        try unknownFields.traverse(visitor: &visitor)
    }

    static func == (lhs: PBLink, rhs: PBLink) -> Bool {
        if lhs.hash != rhs.hash { return false }
        if lhs.name != rhs.name { return false }
        if lhs.tsize != rhs.tsize { return false }
        if lhs.unknownFields != rhs.unknownFields { return false }
        return true
    }
}

// MARK: - PBNode

struct PBNode {
    var data: Data?
    var links: [PBLink] = []

    var unknownFields = SwiftProtobuf.UnknownStorage()

    init() {}
}

extension PBNode: SwiftProtobuf.Message, SwiftProtobuf._MessageImplementationBase, SwiftProtobuf._ProtoNameProviding {
    static let protoMessageName: String = "merkledag.PBNode"
    static let _protobuf_nameMap: SwiftProtobuf._NameMap = [
        1: .same(proto: "Data"),
        2: .same(proto: "Links"),
    ]

    var hasData: Bool { data != nil }

    mutating func decodeMessage(decoder: inout some SwiftProtobuf.Decoder) throws {
        while let fieldNumber = try decoder.nextFieldNumber() {
            switch fieldNumber {
            case 1: try decoder.decodeSingularBytesField(value: &data)
            case 2: try decoder.decodeRepeatedMessageField(value: &links)
            default: break
            }
        }
    }

    func traverse(visitor: inout some SwiftProtobuf.Visitor) throws {
        if let v = data {
            try visitor.visitSingularBytesField(value: v, fieldNumber: 1)
        }
        if !links.isEmpty {
            try visitor.visitRepeatedMessageField(value: links, fieldNumber: 2)
        }
        try unknownFields.traverse(visitor: &visitor)
    }

    static func == (lhs: PBNode, rhs: PBNode) -> Bool {
        if lhs.data != rhs.data { return false }
        if lhs.links != rhs.links { return false }
        if lhs.unknownFields != rhs.unknownFields { return false }
        return true
    }
}
