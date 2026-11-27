/// ROM data handler for NSF bank mapping.
pub struct RomData {
    /// The raw ROM data (without NSF header).
    pub data: Vec<u8>,
    /// Number of valid data bytes.
    pub file_size: i64,
    /// Mask for address wrapping (power of 2 - 1).
    pub mask: i32,
    /// Rounded ROM size (C++ Rom_Data_::size_), set by set_addr.
    pub size: i32,
    /// Address offset matching C++ Rom_Data_::rom_addr.
    rom_addr: i32,
}

const PAD_EXTRA: i32 = 8;

impl RomData {
    pub fn new(_bank_size: usize) -> Self {
        Self {
            data: Vec::new(),
            file_size: 0,
            mask: 0,
            size: 0,
            rom_addr: 0,
        }
    }

    /// Load ROM data from file bytes, stripping the header.
    pub fn load(&mut self, file_data: &[u8], header_size: usize) -> Result<Vec<u8>, &'static str> {
        if file_data.len() <= header_size {
            return Err("Wrong file type for this emulator");
        }

        let mut header = vec![0u8; header_size];
        header.copy_from_slice(&file_data[..header_size]);

        // Store ROM data without header
        self.data = file_data[header_size..].to_vec();
        self.file_size = self.data.len() as i64;

        // Compute mask: round up to next power of 2, minus 1
        if self.file_size > 0 {
            let rounded = (self.file_size as u64).next_power_of_two();
            self.mask = (rounded - 1) as i32;
        } else {
            self.mask = 0;
        }

        self.rom_addr = 0;

        Ok(header)
    }

    /// Set the base address for ROM mapping, matching C++ `rom.set_addr(load_addr % bank_size)`.
    /// This prepends padding to the ROM data so bank mapping works correctly
    /// when load_addr is not aligned to bank_size.
    pub fn set_addr(&mut self, addr: i32, bank_size: i32) {
        let pad_size = (bank_size + PAD_EXTRA) as usize;

        // Match C++ Rom_Data_::set_addr_(addr, unit):
        //   rom_addr = addr - unit - pad_extra;
        self.rom_addr = addr - bank_size - PAD_EXTRA;

        // Compute rounded size matching C++:
        //   long rounded = (addr + file_size_ + unit - 1) / unit * unit;
        let rounded = ((addr as i64 + self.file_size + bank_size as i64 - 1) / bank_size as i64
            * bank_size as i64) as i32;

        self.size = rounded.max(0); // C++ size_ = rounded

        // Compute mask from rounded size
        if rounded > 0 {
            let max_addr = (rounded - 1) as u32;
            let shift = 32 - max_addr.leading_zeros();
            self.mask = if shift > 0 { (1i32 << shift) - 1 } else { 0 };
        } else {
            self.mask = 0;
        }

        // Build new buffer matching C++ layout after load_rom_data_ + set_addr_:
        //   rom[0 .. pad_size):                   padding (zeros)
        //   rom[pad_size .. pad_size+file_size):   file data
        //   rom[pad_size+file_size .. new_size):   padding (zeros)
        // where new_size = rounded - rom_addr + pad_extra
        let new_size = (rounded as i64 - self.rom_addr as i64 + PAD_EXTRA as i64) as usize;
        let old_data = std::mem::take(&mut self.data);
        let file_size = old_data.len();
        let total_size = new_size.max(pad_size + file_size);
        self.data = vec![0u8; total_size];
        self.data[pad_size..pad_size + file_size].copy_from_slice(&old_data);
    }

    /// Mask an address to the ROM size.
    pub fn mask_addr(&self, addr: i32) -> i32 {
        addr & self.mask
    }

    /// Get an offset into self.data for the given masked address.
    /// Matches C++ at_addr: &rom[mask_addr(addr) - rom_addr]
    pub fn at_addr(&self, addr: i32) -> usize {
        let offset = (self.mask_addr(addr) - self.rom_addr) as usize;
        if offset < self.data.len() {
            offset
        } else {
            0 // unmapped (wraps to start)
        }
    }
}
