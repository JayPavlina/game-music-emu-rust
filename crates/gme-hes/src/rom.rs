//! ROM data handler for HES bank mapping.
//! Port of C++ Rom_Data<page_size=0x2000> (Classic_Emu.h / Classic_Emu.cpp)
//! with fill byte 0xFF (HES "unmapped" value).

/// Bank/page unit size.
pub const UNIT: usize = 0x2000;
const PAD_EXTRA: usize = 8;
/// Padding placed before and after file data (unit + pad_extra).
pub const PAD_SIZE: usize = UNIT + PAD_EXTRA;

pub struct RomData {
    /// Padded ROM image:
    /// [fill x pad_size][file data after header][fill x pad_size]
    /// (possibly shrunk by set_addr, mirroring C++ realloc).
    pub data: Vec<u8>,
    /// Size of file data excluding header.
    pub file_size: i64,
    rom_addr: i32,
    mask: i32,
    size_: i32,
}

impl Default for RomData {
    fn default() -> Self {
        Self::new()
    }
}

impl RomData {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            file_size: 0,
            rom_addr: 0,
            mask: 0,
            size_: 0,
        }
    }

    /// C++ Rom_Data_::load_rom_data_( in, header_size, header_out, fill, pad_size ).
    /// Returns the header bytes.
    pub fn load(
        &mut self,
        file: &[u8],
        header_size: usize,
        fill: u8,
    ) -> Result<Vec<u8>, &'static str> {
        let file_offset = PAD_SIZE - header_size;

        self.rom_addr = 0;
        self.mask = 0;
        self.size_ = 0;
        self.data.clear();

        let total = file.len();
        if total <= header_size {
            return Err("wrong file type");
        }

        self.data = vec![0u8; file_offset + total + PAD_SIZE];
        self.data[file_offset..file_offset + total].copy_from_slice(file);

        self.file_size = (total - header_size) as i64;
        let header = file[..header_size].to_vec();

        // memset( rom.begin(), fill, pad_size ) — clobbers the header copy in rom
        for b in &mut self.data[..PAD_SIZE] {
            *b = fill;
        }
        let len = self.data.len();
        for b in &mut self.data[len - PAD_SIZE..] {
            *b = fill;
        }

        Ok(header)
    }

    /// C++ Rom_Data_::set_addr_( addr, unit ).
    pub fn set_addr(&mut self, addr: i32) {
        self.rom_addr = addr - UNIT as i32 - PAD_EXTRA as i32;

        let rounded_l =
            (addr as i64 + self.file_size + UNIT as i64 - 1) / UNIT as i64 * UNIT as i64;
        let rounded = if rounded_l <= 0 {
            self.mask = 0;
            0i64
        } else {
            let max_addr = (rounded_l - 1) as u64;
            let mut shift = 0;
            while (max_addr >> shift) != 0 {
                shift += 1;
            }
            self.mask = ((1i64 << shift) - 1) as i32;
            rounded_l
        };

        self.size_ = rounded as i32;
        // rom.resize( rounded - rom_addr + pad_extra ) — analysis shows this
        // is always <= current size for valid inputs; mirror with truncate and
        // grow-with-zeros fallback (C++ realloc growth would be garbage, but
        // never happens for the layouts we produce).
        let new_len = (rounded - self.rom_addr as i64 + PAD_EXTRA as i64).max(0) as usize;
        if new_len <= self.data.len() {
            self.data.truncate(new_len);
        } else {
            self.data.resize(new_len, 0);
        }
    }

    #[inline]
    pub fn mask_addr(&self, addr: i32) -> i32 {
        addr & self.mask
    }

    /// C++ at_addr: index into `data` of the page starting at `addr`.
    /// Returns 0 (the unmapped fill page) if outside data.
    pub fn at_addr(&self, addr: i32) -> usize {
        let offset = (self.mask_addr(addr).wrapping_sub(self.rom_addr)) as u32;
        if offset > (self.data.len() as i64 - PAD_SIZE as i64).max(0) as u32 {
            0
        } else {
            offset as usize
        }
    }

    /// Index of the unmapped page (filled with the fill byte).
    #[inline]
    pub fn unmapped(&self) -> usize {
        0
    }

    /// Index of the first byte of file data (after the header).
    #[inline]
    pub fn begin_index(&self) -> usize {
        PAD_SIZE
    }
}
