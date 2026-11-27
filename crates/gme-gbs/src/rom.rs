/// ROM data handler for GBS bank mapping.
/// Matches C++ Rom_Data<0x4000> from Classic_Emu.h.
pub struct RomData {
    pub data: Vec<u8>,
    pub file_size: i64,
    rom_addr: i32,
    pub mask: i32,
    size: i32,
    /// Offset of the file bytes within `data` (0 right after load,
    /// `unit + PAD_EXTRA` once set_addr has laid out the padded buffer).
    pad_offset: usize,
}

const PAD_EXTRA: i32 = 8;

impl RomData {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            file_size: 0,
            rom_addr: 0,
            mask: 0,
            size: 0,
            pad_offset: 0,
        }
    }

    /// Load ROM data from file bytes, stripping the header.
    /// Returns the header bytes.
    pub fn load(&mut self, file_data: &[u8], header_size: usize) -> Result<Vec<u8>, &'static str> {
        if file_data.len() <= header_size {
            return Err("Wrong file type for this emulator");
        }

        let mut header = vec![0u8; header_size];
        header.copy_from_slice(&file_data[..header_size]);

        self.data = file_data[header_size..].to_vec();
        self.file_size = self.data.len() as i64;
        self.rom_addr = 0;
        self.mask = 0;
        self.size = 0;
        self.pad_offset = 0;

        Ok(header)
    }

    /// Set the base address for ROM mapping.
    /// Matches C++ Rom_Data_::set_addr_(addr, unit).
    pub fn set_addr(&mut self, addr: i32, unit: i32) {
        let pad_size = (unit + PAD_EXTRA) as usize;

        self.rom_addr = addr - unit - PAD_EXTRA;

        let rounded =
            ((addr as i64 + self.file_size + unit as i64 - 1) / unit as i64 * unit as i64) as i32;

        if rounded > 0 {
            let max_addr = (rounded - 1) as u32;
            let shift = 32 - max_addr.leading_zeros();
            self.mask = if shift > 0 { (1i32 << shift) - 1 } else { 0 };
        } else {
            self.mask = 0;
        }
        self.size = rounded;

        // Build new buffer matching C++ layout:
        //   [0 .. pad_size):                     padding (unmapped page fill)
        //   [pad_size .. pad_size+file_size):     file data
        //   [pad_size+file_size .. new_size):     padding
        // The file bytes are taken from their current offset (pad_offset), so
        // a second call (e.g. restarting a track) is idempotent — C++ pads at
        // load and only resizes here.
        let new_size = (self.size as i64 - self.rom_addr as i64 + PAD_EXTRA as i64) as usize;
        let old_data = std::mem::take(&mut self.data);
        let file_size = self.file_size as usize;
        let total_size = new_size.max(pad_size + file_size);
        self.data = vec![0u8; total_size];
        self.data[pad_size..pad_size + file_size]
            .copy_from_slice(&old_data[self.pad_offset..self.pad_offset + file_size]);
        self.pad_offset = pad_size;
    }

    /// Mask an address to the ROM size.
    pub fn mask_addr(&self, addr: i32) -> i32 {
        addr & self.mask
    }

    /// Size of data + start addr, rounded to a multiple of unit.
    pub fn size(&self) -> i32 {
        self.size
    }

    /// Pointer to unmapped page (first bytes of data).
    pub fn unmapped(&self) -> *const u8 {
        self.data.as_ptr()
    }

    /// Get a pointer to the page starting at addr.
    /// Returns unmapped() if outside data.
    /// Matches C++ at_addr: &rom[mask_addr(addr) - rom_addr]
    pub fn at_addr(&self, addr: i32) -> *const u8 {
        let masked = self.mask_addr(addr);
        let offset = (masked - self.rom_addr) as usize;
        if offset < self.data.len() {
            unsafe { self.data.as_ptr().add(offset) }
        } else {
            self.unmapped()
        }
    }
}

impl Default for RomData {
    fn default() -> Self {
        Self::new()
    }
}
