use core::fmt;
use font8x8::legacy::BASIC_LEGACY;
use spin::Mutex;

const SCALE: usize = 2;
const FONT_WIDTH: usize = 8 * SCALE;
const FONT_HEIGHT: usize = 8 * SCALE;

const MAX_LINES: usize = 1000;
const MAX_COLS: usize = 128;

const BACKBUFFER_SIZE: usize = 2560 * 1600;
static mut BACKBUFFER: [u32; BACKBUFFER_SIZE] = [0; BACKBUFFER_SIZE];

static mut TEXT_BUFFER: [[u8; MAX_COLS]; MAX_LINES] = [[0; MAX_COLS]; MAX_LINES];
static mut LINE_LEN: [usize; MAX_LINES] = [0; MAX_LINES];
static mut TOTAL_LINES: usize = 1;
static mut VIEW_OFFSET: usize = 0;

pub struct Writer {
    fb_ptr: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
}

impl Writer {
    pub fn new(fb_ptr: *mut u8, width: usize, height: usize, pitch: usize) -> Self {
        Self { fb_ptr, width, height, pitch }
    }

    pub fn write_byte(&mut self, byte: u8) {
        unsafe {
            match byte {
                b'\n' => self.newline(),
                _ => {
                    let buf_idx = (TOTAL_LINES - 1) % MAX_LINES;
                    let col = LINE_LEN[buf_idx];
                    if col < MAX_COLS && (col + 1) * FONT_WIDTH <= self.width {
                        TEXT_BUFFER[buf_idx][col] = byte;
                        LINE_LEN[buf_idx] += 1;
                    } else {
                        self.newline();
                        let new_idx = (TOTAL_LINES - 1) % MAX_LINES;
                        TEXT_BUFFER[new_idx][0] = byte;
                        LINE_LEN[new_idx] = 1;
                    }
                }
            }
        }
    }

    fn newline(&mut self) {
        unsafe {
            TOTAL_LINES += 1;
            let buf_idx = (TOTAL_LINES - 1) % MAX_LINES;
            LINE_LEN[buf_idx] = 0;
        }
    }

    pub fn redraw(&mut self) {
        let pitch_u32 = self.pitch / 4;
        let bg_color: u32 = 0xFF0064FF;

        unsafe {
            let backbuffer_ptr = core::ptr::addr_of_mut!(BACKBUFFER).cast::<u32>();
            let backbuffer_slice = core::slice::from_raw_parts_mut(backbuffer_ptr, BACKBUFFER_SIZE);
            backbuffer_slice.fill(bg_color);

            let visible_lines = self.height / FONT_HEIGHT;
            let bottom_line = TOTAL_LINES.saturating_sub(1).saturating_sub(VIEW_OFFSET);
            let start_line = bottom_line.saturating_sub(visible_lines.saturating_sub(1));

            for absolute_y in start_line..=bottom_line {
                let screen_y = absolute_y - start_line;
                let buf_idx = absolute_y % MAX_LINES;
                let len = LINE_LEN[buf_idx];
                for x in 0..len {
                    let byte = TEXT_BUFFER[buf_idx][x];
                    self.draw_char_to_backbuffer(byte, x * FONT_WIDTH, screen_y * FONT_HEIGHT);
                }
            }

            for y in 0..self.height {
                let src = core::ptr::addr_of!(BACKBUFFER).cast::<u32>().add(y * pitch_u32);
                let dst = self.fb_ptr.add(y * self.pitch) as *mut u32;
                core::ptr::copy_nonoverlapping(src, dst, self.width);
            }
        }
    }

    fn draw_char_to_backbuffer(&self, byte: u8, px: usize, py: usize) {
        let glyph_index = if byte < 128 { byte as usize } else { 0x3f };
        let glyph = BASIC_LEGACY[glyph_index];
        let pitch_u32 = self.pitch / 4;

        unsafe {
            let backbuffer_ptr = core::ptr::addr_of_mut!(BACKBUFFER).cast::<u32>();
            for (y, &row) in glyph.iter().enumerate() {
                for x in 0..8 {
                    if (row & (1 << x)) != 0 {
                        for sy in 0..SCALE {
                            for sx in 0..SCALE {
                                let draw_x = px + (x * SCALE) + sx;
                                let draw_y = py + (y * SCALE) + sy;
                                if draw_x < self.width && draw_y < self.height {
                                    let offset = draw_y * pitch_u32 + draw_x;
                                    if offset < BACKBUFFER_SIZE {
                                        backbuffer_ptr.add(offset).write(0xFFFFFFFF);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0x3f),
            }
        }
        Ok(())
    }
}

pub static WRITER: Mutex<Option<Writer>> = Mutex::new(None);

pub fn init_writer(fb_ptr: *mut u8, width: usize, height: usize, pitch: usize) {
    *WRITER.lock() = Some(Writer::new(fb_ptr, width, height, pitch));
}

#[macro_export]
macro_rules! print { ($($arg:tt)*) => ($crate::writer::_print(format_args!($($arg)*))); }

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.write_fmt(args).unwrap();
        unsafe { VIEW_OFFSET = 0; }
        writer.redraw();
    }
}

pub fn scroll_up() {
    if let Some(writer) = WRITER.lock().as_mut() {
        let visible_lines = writer.height / FONT_HEIGHT;
        unsafe {
            let max_scroll = TOTAL_LINES.saturating_sub(visible_lines);
            if VIEW_OFFSET < max_scroll {
                VIEW_OFFSET += 5;
                if VIEW_OFFSET > max_scroll { VIEW_OFFSET = max_scroll; }
                writer.redraw();
            }
        }
    }
}

pub fn scroll_down() {
    if let Some(writer) = WRITER.lock().as_mut() {
        unsafe { VIEW_OFFSET = VIEW_OFFSET.saturating_sub(5); }
        writer.redraw();
    }
}

unsafe impl Send for Writer {}
unsafe impl Sync for Writer {}
