use crate::syscalls::sys_write;

#[repr(align(4096))]
pub struct Align4096<T>(pub T);

// ------------------------------------------------------------
// Minimal no_std String Buffer Formatter Helper
// ------------------------------------------------------------

pub struct StrbufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> StrbufWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn as_str(&self) -> &str {
        if let Ok(s) = core::str::from_utf8(&self.buf[0..self.pos]) {
            s
        } else {
            ""
        }
    }
}

impl<'a> core::fmt::Write for StrbufWriter<'a> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remain = self.buf.len() - self.pos;
        let to_copy = core::cmp::min(remain, bytes.len());
        if to_copy > 0 {
            self.buf[self.pos..self.pos + to_copy].copy_from_slice(&bytes[0..to_copy]);
            self.pos += to_copy;
        }
        Ok(())
    }
}

pub fn serial_print(s: &str) {
    let _ = sys_write(2, s.as_ptr(), s.len());
}
