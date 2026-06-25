use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoundedReadError {
    TooLarge,
    Unreadable,
}

pub(super) fn read_file_bounded(
    path: &Path,
    max_file_bytes: u64,
) -> Result<Vec<u8>, BoundedReadError> {
    let file = fs::File::open(path).map_err(|_| BoundedReadError::Unreadable)?;
    read_bounded(file, max_file_bytes)
}

fn read_bounded<R: Read>(reader: R, max_file_bytes: u64) -> Result<Vec<u8>, BoundedReadError> {
    let read_limit = max_file_bytes
        .checked_add(1)
        .ok_or(BoundedReadError::TooLarge)?;
    let mut bytes = Vec::new();
    reader
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| BoundedReadError::Unreadable)?;

    let actual_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if actual_size > max_file_bytes {
        return Err(BoundedReadError::TooLarge);
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io;

    struct CountingReader<'a> {
        emitted: &'a Cell<usize>,
        total: usize,
    }

    impl Read for CountingReader<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let remaining = self.total.saturating_sub(self.emitted.get());
            if remaining == 0 {
                return Ok(0);
            }
            let count = remaining.min(buffer.len());
            buffer[..count].fill(b'x');
            self.emitted.set(self.emitted.get() + count);
            Ok(count)
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic read failure"))
        }
    }

    #[test]
    fn bounded_read_accepts_exact_limit() {
        let bytes = read_bounded(io::Cursor::new(vec![b'x'; 8]), 8).expect("read exact limit");

        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn bounded_read_rejects_limit_plus_one() {
        let error =
            read_bounded(io::Cursor::new(vec![b'x'; 9]), 8).expect_err("limit plus one must fail");

        assert_eq!(error, BoundedReadError::TooLarge);
    }

    #[test]
    fn zero_limit_accepts_empty_and_rejects_one_byte() {
        let bytes = read_bounded(io::Cursor::new(Vec::<u8>::new()), 0).expect("read empty file");
        assert!(bytes.is_empty());

        let error =
            read_bounded(io::Cursor::new(vec![b'x']), 0).expect_err("one byte must be too large");
        assert_eq!(error, BoundedReadError::TooLarge);
    }

    #[test]
    fn read_errors_are_unreadable() {
        let error = read_bounded(FailingReader, 8).expect_err("reader failure must fail");

        assert_eq!(error, BoundedReadError::Unreadable);
    }

    #[test]
    fn bounded_read_stops_after_limit_plus_one() {
        let emitted = Cell::new(0usize);
        let reader = CountingReader {
            emitted: &emitted,
            total: 1024,
        };

        let error = read_bounded(reader, 8).expect_err("oversized reader must fail");

        assert_eq!(error, BoundedReadError::TooLarge);
        assert_eq!(emitted.get(), 9);
    }

    #[test]
    fn max_u64_limit_is_rejected_without_reading() {
        let emitted = Cell::new(0usize);
        let reader = CountingReader {
            emitted: &emitted,
            total: 1,
        };

        let error = read_bounded(reader, u64::MAX).expect_err("overflowing limit must fail");

        assert_eq!(error, BoundedReadError::TooLarge);
        assert_eq!(emitted.get(), 0);
    }
}
