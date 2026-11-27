// Expansion chip stubs - to be fully implemented later.

/// Trait for NES expansion sound chips.
pub trait ExpansionChip {
    fn reset(&mut self);
    fn end_frame(&mut self, time: i32);
    fn write(&mut self, time: i32, addr: u16, data: u8);
    fn read(&mut self, time: i32, addr: u16) -> u8 {
        let _ = (time, addr);
        0
    }
}

pub struct StubChip;
impl ExpansionChip for StubChip {
    fn reset(&mut self) {}
    fn end_frame(&mut self, _time: i32) {}
    fn write(&mut self, _time: i32, _addr: u16, _data: u8) {}
}
