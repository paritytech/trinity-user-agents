/* SPDX-License-Identifier: Apache-2.0 OR MIT
 * Derived from paritytech/polkavm examples/quake/src/keys.rs at
 * 3df1d0309c4c81a1aad0a755d83570d203bba1d9.
 */

pub const UPARROW: u8 = 0x80;
pub const DOWNARROW: u8 = 0x81;
pub const RIGHTARROW: u8 = 0x82;
pub const LEFTARROW: u8 = 0x83;
pub const LSHIFT: u8 = 0x9a;
pub const RSHIFT: u8 = 0x9b;
pub const LCTRL: u8 = 0x9c;
pub const RCTRL: u8 = 0x9d;
pub const LALT: u8 = 0x9e;
pub const RALT: u8 = 0x9f;
pub const MOUSE_1: u8 = 0xa0;
pub const MOUSE_2: u8 = 0xa1;
pub const MOUSE_3: u8 = 0xa2;
pub const MOUSE_X: u8 = 0xa3;
pub const MOUSE_Y: u8 = 0xa4;

pub fn from_hid(code: u8) -> Option<u8> {
    Some(match code {
        0x04..=0x1d => b'a' + (code - 0x04),
        0x1e..=0x26 => b'1' + (code - 0x1e),
        0x27 => b'0',
        0x28 | 0x58 => b'\n',
        0x29 => 0x1b,
        0x2a => 0x08,
        0x2b => b'\t',
        0x2c => b' ',
        0x2d | 0x56 => b'-',
        0x2e => b'=',
        0x2f => b'[',
        0x30 => b']',
        0x31 => b'\\',
        0x33 => b';',
        0x34 => b'\'',
        0x35 => b'`',
        0x36 => b',',
        0x37 | 0x63 => b'.',
        0x38 | 0x54 => b'/',
        0x3a..=0x45 => 0x84 + (code - 0x3a),
        0x46 => 0x91,
        0x47 => 0x92,
        0x48 => 0x93,
        0x49 => 0x94,
        0x4a => 0x96,
        0x4b => 0x98,
        0x4c => 0x95,
        0x4d => 0x97,
        0x4e => 0x99,
        0x4f => RIGHTARROW,
        0x50 => LEFTARROW,
        0x51 => DOWNARROW,
        0x52 => UPARROW,
        0x55 => b'*',
        0x57 => b'+',
        0x59 => 0x97,
        0x5a => DOWNARROW,
        0x5b => 0x99,
        0x5c => LEFTARROW,
        0x5d => b'5',
        0x5e => RIGHTARROW,
        0x5f => 0x96,
        0x60 => UPARROW,
        0x61 => 0x98,
        0x62 => b'.',
        0xe0 => LCTRL,
        0xe1 => LSHIFT,
        0xe2 => LALT,
        0xe4 => RCTRL,
        0xe5 => RSHIFT,
        0xe6 => RALT,
        _ => return None,
    })
}

pub fn from_button(code: u8) -> Option<u8> {
    Some(match code {
        1 => MOUSE_1,
        2 => MOUSE_2,
        3 => MOUSE_3,
        _ => return None,
    })
}
