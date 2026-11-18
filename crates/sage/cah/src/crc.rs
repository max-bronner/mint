#[derive(Debug, Clone, Copy)]
pub struct Crc32 {
    state: u32,
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

impl Crc32 {
    #[must_use]
    pub fn new() -> Self {
        Self { state: !0 }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.state ^= u32::from(byte);
            for _ in 0..8 {
                let carry = self.state & 1;
                self.state >>= 1;
                if carry != 0 {
                    self.state ^= 0xEDB8_8320;
                }
            }
        }
    }

    #[must_use]
    pub fn finish(&self) -> u32 {
        !self.state
    }
}
