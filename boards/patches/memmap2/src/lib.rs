//! Minimal `memmap2` API for targets without `mmap` (ESP-IDF): [`Mmap::map`] reads the whole
//! file into memory. Only what fontique uses is provided.

use std::fs::File;
use std::io::{self, Read};
use std::ops::Deref;

/// A file's contents, read into memory rather than mapped.
pub struct Mmap {
    data: Vec<u8>,
}

impl Mmap {
    /// Reads `file` from its current position to the end.
    ///
    /// # Safety
    /// Always safe here; `unsafe` only mirrors memmap2's signature.
    pub unsafe fn map(file: &File) -> io::Result<Mmap> {
        let mut data = Vec::new();
        (&*file).read_to_end(&mut data)?;
        Ok(Mmap { data })
    }
}

impl Deref for Mmap {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.data
    }
}

impl AsRef<[u8]> for Mmap {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}
