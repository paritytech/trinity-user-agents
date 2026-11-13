#!/usr/bin/env python3
# Writes the CAR fixtures of truapi-host-cli's published_worker tests, from the
# CARv1, CID, DAG-PB and UnixFS specs.
import hashlib, sys, os
out = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))
def uv(n):
    b = bytearray()
    while True:
        x = n & 0x7f; n >>= 7
        if n: b.append(x | 0x80)
        else: b.append(x); return bytes(b)
def field(num, wire, payload):
    key = uv(num << 3 | wire)
    return key + (uv(len(payload)) + payload if wire == 2 else uv(payload))
def cid(codec, data, v0=False):
    mh = b'\x12\x20' + hashlib.sha256(data).digest()
    return mh if v0 else b'\x01' + uv(codec) + mh
RAW, PB = 0x55, 0x70
def unixfs(kind, data=None, blocksizes=()):
    b = field(1, 0, kind)
    if data is not None: b += field(2, 2, data)
    if kind == 2: b += field(3, 0, sum(blocksizes) + (len(data) if data else 0))
    for s in blocksizes: b += field(4, 0, s)
    return b
def pbnode(links, data):
    b = b''
    for (h, name, size) in links:
        l = field(1, 2, h)
        if name is not None: l += field(2, 2, name.encode())
        l += field(3, 0, size)
        b += field(2, 2, l)
    return b + field(1, 2, data)
def car(root, blocks):
    root_cbor = b'\xd8\x2a\x58' + bytes([len(root) + 1]) + b'\x00' + root
    header = b'\xa2\x65roots\x81' + root_cbor + b'\x67version\x01'
    body = b''.join(uv(len(c) + len(d)) + c + d for c, d in blocks)
    return uv(len(header)) + header + body

def directory(index_parts, util, extra_name=None):
    blocks = []
    leaves = []
    for part in index_parts:
        c = cid(RAW, part); blocks.append((c, part)); leaves.append((c, None, len(part)))
    index = pbnode(leaves, unixfs(2, None, [len(p) for p in index_parts]))
    index_cid = cid(PB, index); blocks.append((index_cid, index))
    util_node = pbnode([], unixfs(2, util))
    util_cid = cid(PB, util_node, v0=True); blocks.append((util_cid, util_node))
    lib = pbnode([(util_cid, 'util.js', len(util_node))], unixfs(1))
    lib_cid = cid(PB, lib); blocks.append((lib_cid, lib))
    links = [(index_cid, 'index.js', len(index)), (lib_cid, extra_name or 'lib', len(lib))]
    root = pbnode(links, unixfs(1))
    root_cid = cid(PB, root); blocks.append((root_cid, root))
    return car(root_cid, blocks[::-1])

dir_car = directory([b'import { util } from "./lib/util.js";\n', b'util();\n'], b'export const util = () => 1;\n')
open(os.path.join(out, 'directory.car'), 'wb').write(dir_car)
half = len(dir_car) // 2
parts = [dir_car[:half], dir_car[half:]]
blocks = [(cid(RAW, p), p) for p in parts]
file_node = pbnode([(c, None, len(p)) for c, p in blocks], unixfs(2, None, [len(p) for p in parts]))
file_cid = cid(PB, file_node)
open(os.path.join(out, 'uploaded.car'), 'wb').write(car(file_cid, [(file_cid, file_node)] + blocks))
open(os.path.join(out, 'escape.car'), 'wb').write(directory([b'a'], b'b', extra_name='..'))
print(file_cid.hex())
