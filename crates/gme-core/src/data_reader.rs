// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2005-2006 Shay Green
// Rust adaptation of Data_Reader.cpp (2026-06-09), license notice updated 2026-11-27.
// Distributed without warranty. See COPYING.LESSER in this crate.
use crate::error::{GmeError, GmeResult};
use std::fs::File;
use std::io::{Read, SeekFrom};

/// Trait for reading data with remaining-bytes tracking.
/// Replaces the C++ Data_Reader abstract class.
pub trait DataReader {
    /// Read up to `count` bytes, returning the number actually read.
    /// Returns 0 on EOF, negative on error.
    fn read_avail(&mut self, buf: &mut [u8]) -> i64;

    /// Read exactly `count` bytes. Returns error if not enough data.
    fn read(&mut self, buf: &mut [u8]) -> GmeResult {
        let count = buf.len() as i64;
        debug_assert!(count > 0);
        let result = self.read_avail(buf);
        if result != count {
            if result >= 0 && result < count {
                return Err(GmeError::Eof);
            }
            return Err(GmeError::ReadError);
        }
        Ok(())
    }

    /// Number of bytes remaining.
    fn remain(&self) -> i64;

    /// Skip `count` bytes by reading and discarding them.
    fn skip(&mut self, mut count: i64) -> GmeResult {
        debug_assert!(count >= 0);
        let mut buf = [0u8; 512];
        while count > 0 {
            let n = (buf.len() as i64).min(count);
            count -= n;
            self.read(&mut buf[..n as usize])?;
        }
        Ok(())
    }
}

/// Trait for seekable data readers. Replaces C++ File_Reader.
pub trait FileReader: DataReader {
    /// Total size of the file/data.
    fn size(&self) -> i64;

    /// Current read position.
    fn tell(&mut self) -> i64;

    /// Seek to absolute position.
    fn seek(&mut self, pos: i64) -> GmeResult;

    /// Default remain for seekable readers: size - tell.
    fn file_remain(&mut self) -> i64 {
        self.size() - self.tell()
    }

    /// Default skip for seekable readers: just seek forward.
    fn file_skip(&mut self, n: i64) -> GmeResult {
        debug_assert!(n >= 0);
        if n == 0 {
            return Ok(());
        }
        let pos = self.tell() + n;
        self.seek(pos)
    }
}

/// Disk file reader. Replaces C++ Std_File_Reader.
pub struct StdFileReader {
    file: File,
    size: i64,
    pos: i64,
}

impl StdFileReader {
    pub fn open(path: &str) -> GmeResult<Self> {
        let file = File::open(path).map_err(|_| GmeError::OpenFailed)?;
        let metadata = file.metadata().map_err(|_| GmeError::OpenFailed)?;
        Ok(Self {
            file,
            size: metadata.len() as i64,
            pos: 0,
        })
    }
}

impl DataReader for StdFileReader {
    fn read_avail(&mut self, buf: &mut [u8]) -> i64 {
        match self.file.read(buf) {
            Ok(n) => {
                self.pos += n as i64;
                n as i64
            }
            Err(_) => -1,
        }
    }

    fn remain(&self) -> i64 {
        self.size - self.pos
    }

    fn skip(&mut self, n: i64) -> GmeResult {
        self.file_skip(n)
    }
}

impl FileReader for StdFileReader {
    fn size(&self) -> i64 {
        self.size
    }

    fn tell(&mut self) -> i64 {
        self.pos
    }

    fn seek(&mut self, pos: i64) -> GmeResult {
        use std::io::Seek;
        self.file
            .seek(SeekFrom::Start(pos as u64))
            .map_err(|_| GmeError::Eof)?;
        self.pos = pos;
        Ok(())
    }
}

/// Memory-backed file reader. Replaces C++ Mem_File_Reader.
pub struct MemFileReader {
    data: Vec<u8>,
    pos: i64,
}

impl MemFileReader {
    pub fn new(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            pos: 0,
        }
    }

    pub fn from_vec(data: Vec<u8>) -> Self {
        Self { data, pos: 0 }
    }
}

impl DataReader for MemFileReader {
    fn read_avail(&mut self, buf: &mut [u8]) -> i64 {
        let remain = self.remain();
        let count = (buf.len() as i64).min(remain).max(0) as usize;
        if count == 0 {
            return 0;
        }
        let start = self.pos as usize;
        buf[..count].copy_from_slice(&self.data[start..start + count]);
        self.pos += count as i64;
        count as i64
    }

    fn remain(&self) -> i64 {
        self.size() - self.pos
    }

    fn skip(&mut self, n: i64) -> GmeResult {
        self.file_skip(n)
    }
}

impl FileReader for MemFileReader {
    fn size(&self) -> i64 {
        self.data.len() as i64
    }

    fn tell(&mut self) -> i64 {
        self.pos
    }

    fn seek(&mut self, pos: i64) -> GmeResult {
        debug_assert!(pos >= 0);
        if pos > self.size() {
            return Err(GmeError::Eof);
        }
        self.pos = pos;
        Ok(())
    }
}

/// Limits reading to a subset of another reader.
/// Replaces C++ Subset_Reader.
pub struct SubsetReader<'a> {
    inner: &'a mut dyn DataReader,
    remain: i64,
}

impl<'a> SubsetReader<'a> {
    pub fn new(reader: &'a mut dyn DataReader, size: i64) -> Self {
        let remain = reader.remain().min(size).max(0);
        Self {
            inner: reader,
            remain,
        }
    }
}

impl<'a> DataReader for SubsetReader<'a> {
    fn read_avail(&mut self, buf: &mut [u8]) -> i64 {
        let count = (buf.len() as i64).min(self.remain).max(0) as usize;
        if count == 0 {
            return 0;
        }
        self.remain -= count as i64;
        self.inner.read_avail(&mut buf[..count])
    }

    fn remain(&self) -> i64 {
        self.remain
    }
}

/// Joins already-read header bytes with remaining data from a reader.
/// Avoids seeking back to re-read the header.
/// Replaces C++ Remaining_Reader.
pub struct RemainingReader<'a> {
    header: &'a [u8],
    header_pos: usize,
    inner: &'a mut dyn DataReader,
}

impl<'a> RemainingReader<'a> {
    pub fn new(header: &'a [u8], reader: &'a mut dyn DataReader) -> Self {
        Self {
            header,
            header_pos: 0,
            inner: reader,
        }
    }

    fn header_remaining(&self) -> i64 {
        (self.header.len() - self.header_pos) as i64
    }

    fn read_header(&mut self, buf: &mut [u8]) -> i64 {
        let avail = self.header_remaining();
        let count = (buf.len() as i64).min(avail).max(0) as usize;
        if count == 0 {
            return 0;
        }
        buf[..count].copy_from_slice(&self.header[self.header_pos..self.header_pos + count]);
        self.header_pos += count;
        count as i64
    }
}

impl<'a> DataReader for RemainingReader<'a> {
    fn read_avail(&mut self, buf: &mut [u8]) -> i64 {
        let count = (buf.len() as i64).min(self.remain()).max(0) as usize;
        if count == 0 {
            return 0;
        }
        let first = self.read_header(buf);
        let remaining = count - first as usize;
        if remaining > 0 {
            let second = self.inner.read_avail(&mut buf[first as usize..count]);
            if second <= 0 {
                return second;
            }
            first + second
        } else {
            first
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> GmeResult {
        let count = buf.len() as i64;
        debug_assert!(count > 0);
        let first = self.read_header(buf);
        let remaining = count - first;
        if remaining == 0 {
            return Ok(());
        }
        self.inner.read(&mut buf[first as usize..count as usize])
    }

    fn remain(&self) -> i64 {
        self.header_remaining() + self.inner.remain()
    }
}

/// Reads data via a callback function.
/// Replaces C++ Callback_Reader.
pub struct CallbackReader<F: FnMut(&mut [u8]) -> i64> {
    callback: F,
    remain: i64,
}

impl<F: FnMut(&mut [u8]) -> i64> CallbackReader<F> {
    pub fn new(callback: F, size: i64) -> Self {
        Self {
            callback,
            remain: size.max(0),
        }
    }
}

impl<F: FnMut(&mut [u8]) -> i64> DataReader for CallbackReader<F> {
    fn read_avail(&mut self, buf: &mut [u8]) -> i64 {
        let count = (buf.len() as i64).min(self.remain).max(0) as usize;
        if count == 0 {
            return 0;
        }
        let result = (self.callback)(&mut buf[..count]);
        if result < 0 {
            return -1;
        }
        self.remain -= result;
        result
    }

    fn remain(&self) -> i64 {
        self.remain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mem_file_reader_basic() {
        let data = b"hello world";
        let mut reader = MemFileReader::new(data);
        assert_eq!(reader.size(), 11);
        assert_eq!(reader.tell(), 0);
        assert_eq!(reader.remain(), 11);

        let mut buf = [0u8; 5];
        let n = reader.read_avail(&mut buf);
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
        assert_eq!(reader.tell(), 5);
        assert_eq!(reader.remain(), 6);
    }

    #[test]
    fn test_mem_file_reader_seek() {
        let data = b"hello world";
        let mut reader = MemFileReader::new(data);
        reader.seek(6).unwrap();
        let mut buf = [0u8; 5];
        reader.read(&mut buf).unwrap();
        assert_eq!(&buf, b"world");
    }

    #[test]
    fn test_mem_file_reader_eof() {
        let data = b"hi";
        let mut reader = MemFileReader::new(data);
        let mut buf = [0u8; 5];
        let n = reader.read_avail(&mut buf);
        assert_eq!(n, 2);
        assert!(reader.read(&mut [0u8; 1]).is_err());
    }

    #[test]
    fn test_subset_reader() {
        let data = b"abcdefghij";
        let mut reader = MemFileReader::new(data);
        reader.seek(2).unwrap();
        {
            let mut subset = SubsetReader::new(&mut reader, 4);
            assert_eq!(subset.remain(), 4);
            let mut buf = [0u8; 4];
            subset.read(&mut buf).unwrap();
            assert_eq!(&buf, b"cdef");
            assert_eq!(subset.remain(), 0);
        }
        assert_eq!(reader.tell(), 6);
    }

    #[test]
    fn test_remaining_reader() {
        let header = b"he";
        let data = b"llo world";
        let mut reader = MemFileReader::new(data);
        {
            let mut remaining = RemainingReader::new(header, &mut reader);
            assert_eq!(remaining.remain(), 11); // 2 + 9
            let mut buf = [0u8; 11];
            remaining.read(&mut buf).unwrap();
            assert_eq!(&buf, b"hello world");
        }
    }

    #[test]
    fn test_skip() {
        let data = b"abcdefghij";
        let mut reader = MemFileReader::new(data);
        reader.skip(3).unwrap();
        assert_eq!(reader.tell(), 3);
        let mut buf = [0u8; 1];
        reader.read(&mut buf).unwrap();
        assert_eq!(buf[0], b'd');
    }
}
