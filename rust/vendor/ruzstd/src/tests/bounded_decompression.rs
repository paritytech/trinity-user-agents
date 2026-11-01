// Local behavioral coverage for upstream d3907c9 (which contains no tests).
use crate::blocks::sequence_section::Sequence;
use crate::decoding::block_decoder;
use crate::decoding::errors::{DecodeBlockContentError, DecompressBlockError, ExecuteSequencesError};
use crate::decoding::scratch::DecoderScratch;
use crate::decoding::sequence_execution::execute_sequences;

#[test]
fn bounded_decompression_stops_accumulating_sequences() {
    let mut scratch = DecoderScratch::new(128 * 1024);
    scratch.buffer.push(b"x");
    // Each match is legal in isolation. Their aggregate exceeds one block.
    scratch.sequences.extend_from_slice(&[
        Sequence { ll: 0, ml: 65536, of: 4 },
        Sequence { ll: 0, ml: 65536, of: 4 },
        Sequence { ll: 0, ml: 3, of: 4 },
        Sequence { ll: 0, ml: 65536, of: 4 },
    ]);
    assert!(matches!(
        execute_sequences(&mut scratch, 128 * 1024),
        Err(ExecuteSequencesError::TooManyBytesGenerated {
            max: 131072,
            decoded: 131075,
        })
    ));
    // Upstream checks after each sequence; it must never execute the fourth.
    assert_eq!(scratch.buffer.len(), 131076);
}

#[test]
fn bounded_decompression_accepts_exact_sequence_limit() {
    let mut scratch = DecoderScratch::new(1024);
    scratch.literals_buffer.push(b'x');
    scratch.sequences.push(Sequence { ll: 1, ml: 1023, of: 4 });
    execute_sequences(&mut scratch, 1024).unwrap();
    assert_eq!(scratch.buffer.len(), 1024);
}

#[test]
fn bounded_decompression_raw_and_rle_respect_window() {
    for block_type in [0u32, 1] {
        for size in [1024u32, 1025] {
            let mut decoder = block_decoder::new();
            let mut scratch = DecoderScratch::new(1024);
            let encoded = ((size << 3) | (block_type << 1) | 1).to_le_bytes();
            let (header, _) = decoder.read_block_header(&encoded[..3]).unwrap();
            let body = alloc::vec![b'x'; if block_type == 1 { 1 } else { size as usize }];
            let result = decoder.decode_block_content(&header, &mut scratch, body.as_slice());
            if size == 1024 {
                assert_eq!(result.unwrap(), body.len() as u64);
                assert_eq!(scratch.buffer.len(), 1024);
            } else {
                assert!(matches!(result,
                    Err(DecodeBlockContentError::DecompressBlockError(
                        DecompressBlockError::DecompressedSizeTooLarge {
                            max: 1024, at_least: 1025
                        }
                    ))
                ));
                assert_eq!(scratch.buffer.len(), 0);
            }
        }
    }
}
