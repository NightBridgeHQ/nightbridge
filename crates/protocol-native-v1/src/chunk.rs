//! Chunk planning and resume math for native transfers.

use crate::{NativeError, Result};

/// Default chunk size for native transfers: 1 MiB.
pub const DEFAULT_CHUNK_SIZE: u64 = 1024 * 1024;

/// A byte range belonging to a file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkRange {
    /// Byte offset from the start of the file.
    pub offset: u64,
    /// Number of bytes in the range.
    pub length: u64,
}

impl ChunkRange {
    /// Return the exclusive end offset.
    pub fn end(self) -> Result<u64> {
        self.offset
            .checked_add(self.length)
            .ok_or_else(|| NativeError::Protocol("chunk range overflows u64".to_string()))
    }
}

/// Plan fixed-size chunks for a file.
pub fn plan_chunks(file_size: u64, chunk_size: u64) -> Result<Vec<ChunkRange>> {
    validate_chunk_size(chunk_size)?;

    let mut chunks = Vec::new();
    let mut offset = 0_u64;
    while offset < file_size {
        let length = chunk_size.min(file_size - offset);
        chunks.push(ChunkRange { offset, length });
        offset += length;
    }

    Ok(chunks)
}

/// Return chunk-aligned ranges that are not fully covered by verified ranges.
pub fn plan_missing_chunks(
    file_size: u64,
    chunk_size: u64,
    verified: &[ChunkRange],
) -> Result<Vec<ChunkRange>> {
    validate_verified_ranges(file_size, verified)?;

    let verified = merge_ranges(verified.to_vec())?;
    plan_chunks(file_size, chunk_size).map(|chunks| {
        chunks.into_iter().filter(|chunk| !range_fully_covered(*chunk, &verified)).collect()
    })
}

/// Return whether the verified ranges cover the complete file.
pub fn ranges_cover_file(file_size: u64, verified: &[ChunkRange]) -> Result<bool> {
    validate_verified_ranges(file_size, verified)?;
    if file_size == 0 {
        return Ok(true);
    }

    Ok(merge_ranges(verified.to_vec())? == vec![ChunkRange { offset: 0, length: file_size }])
}

/// Hash bytes with BLAKE3 and encode the digest as lowercase hex.
pub fn blake3_hex(bytes: impl AsRef<[u8]>) -> String {
    blake3::hash(bytes.as_ref()).to_string()
}

fn validate_chunk_size(chunk_size: u64) -> Result<()> {
    if chunk_size == 0 {
        return Err(NativeError::Protocol("chunk size must be greater than zero".to_string()));
    }
    Ok(())
}

fn validate_verified_ranges(file_size: u64, ranges: &[ChunkRange]) -> Result<()> {
    for range in ranges {
        if range.length == 0 {
            return Err(NativeError::Protocol(
                "verified range length must be greater than zero".to_string(),
            ));
        }
        if range.end()? > file_size {
            return Err(NativeError::Protocol("verified range extends past file size".to_string()));
        }
    }
    Ok(())
}

fn merge_ranges(mut ranges: Vec<ChunkRange>) -> Result<Vec<ChunkRange>> {
    ranges.sort_by_key(|range| range.offset);
    let mut merged: Vec<ChunkRange> = Vec::new();

    for range in ranges {
        let Some(last) = merged.last_mut() else {
            merged.push(range);
            continue;
        };
        let last_end = last.end()?;
        let range_end = range.end()?;
        if range.offset <= last_end {
            last.length = range_end.max(last_end) - last.offset;
        } else {
            merged.push(range);
        }
    }

    Ok(merged)
}

fn range_fully_covered(range: ChunkRange, verified: &[ChunkRange]) -> bool {
    let Ok(range_end) = range.end() else {
        return false;
    };
    verified.iter().any(|verified| {
        verified.offset <= range.offset
            && verified.end().map(|verified_end| verified_end >= range_end).unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use crate::chunk::{
        blake3_hex, plan_chunks, plan_missing_chunks, ranges_cover_file, ChunkRange,
    };

    #[test]
    fn plans_chunk_boundaries() {
        assert_eq!(
            plan_chunks(10, 4).unwrap(),
            vec![
                ChunkRange { offset: 0, length: 4 },
                ChunkRange { offset: 4, length: 4 },
                ChunkRange { offset: 8, length: 2 },
            ]
        );
    }

    #[test]
    fn empty_file_has_no_chunks_and_is_complete() {
        assert_eq!(plan_chunks(0, 4).unwrap(), Vec::<ChunkRange>::new());
        assert!(ranges_cover_file(0, &[]).unwrap());
    }

    #[test]
    fn rejects_zero_chunk_size() {
        assert!(plan_chunks(10, 0).is_err());
    }

    #[test]
    fn resume_math_returns_uncovered_chunks() {
        let verified = [ChunkRange { offset: 4, length: 4 }];

        assert_eq!(
            plan_missing_chunks(12, 4, &verified).unwrap(),
            vec![ChunkRange { offset: 0, length: 4 }, ChunkRange { offset: 8, length: 4 }]
        );
    }

    #[test]
    fn partial_verified_chunk_is_requested_again() {
        let verified = [ChunkRange { offset: 0, length: 3 }];

        assert_eq!(
            plan_missing_chunks(8, 4, &verified).unwrap(),
            vec![ChunkRange { offset: 0, length: 4 }, ChunkRange { offset: 4, length: 4 }]
        );
    }

    #[test]
    fn merged_verified_ranges_can_cover_file() {
        let verified = [ChunkRange { offset: 4, length: 6 }, ChunkRange { offset: 0, length: 4 }];

        assert!(ranges_cover_file(10, &verified).unwrap());
    }

    #[test]
    fn blake3_digest_is_lowercase_hex() {
        assert_eq!(
            blake3_hex(b"localsend-improvements"),
            "5ca63dffaad98e99dc8a27e785a8740ea86fc9bafb079ce7550d6d950cc1fd6f"
        );
    }

    proptest! {
        #[test]
        fn planned_chunks_cover_file(file_size in 0_u64..10_000_000, chunk_size in 1_u64..1_000_000) {
            let chunks = plan_chunks(file_size, chunk_size).unwrap();
            let total: u64 = chunks.iter().map(|chunk| chunk.length).sum();
            prop_assert_eq!(total, file_size);
            for pair in chunks.windows(2) {
                prop_assert_eq!(pair[0].offset + pair[0].length, pair[1].offset);
            }
            prop_assert!(chunks.iter().all(|chunk| chunk.length <= chunk_size));
        }
    }
}
