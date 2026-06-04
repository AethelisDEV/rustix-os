//! # PS/2 Mouse Hardware Driver
//!
//! Configures the Intel 8042 Keyboard Controller auxiliary interface to communicate
//! with standard PS/2 mouse devices over IRQ 12. Decodes incoming data packages
//! in a 3-byte state machine to track pixel coordinate deltas and button states.

use x86_64::instructions::port::Port;
use core::sync::atomic::{AtomicI32, AtomicBool, Ordering};

/// Screen width bound for clamping mouse coordinates.
pub const SCREEN_WIDTH: i32 = 1280;
/// Screen height bound for clamping mouse coordinates.
pub const SCREEN_HEIGHT: i32 = 720;

/// Atomic X coordinate of the mouse cursor, clamped between 0 and SCREEN_WIDTH - 1.
pub static MOUSE_X: AtomicI32 = AtomicI32::new(640);

/// Atomic Y coordinate of the mouse cursor, clamped between 0 and SCREEN_HEIGHT - 1.
pub static MOUSE_Y: AtomicI32 = AtomicI32::new(360);

/// Atomic state representing whether the mouse Left Button is currently pressed.
pub static MOUSE_LEFT_CLICKED: AtomicBool = AtomicBool::new(false);

/// Atomic state representing whether the mouse Right Button is currently pressed.
pub static MOUSE_RIGHT_CLICKED: AtomicBool = AtomicBool::new(false);

// State machine trackers for multi-byte PS/2 packet decoding
static mut MOUSE_CYCLE: u8 = 0;
static mut MOUSE_PACKET: [u8; 3] = [0; 3];

/// Helper to wait until the 8042 Keyboard Controller's input buffer is empty.
/// Necessary before writing command or data bytes to hardware ports.
fn mouse_wait_write() {
    let mut status_port: Port<u8> = Port::new(0x64);
    for _ in 0..100_000 {
        unsafe {
            if (status_port.read() & 2) == 0 {
                return;
            }
        }
    }
}

/// Helper to wait until the 8042 Keyboard Controller's output buffer is full.
/// Necessary before reading data bytes from port 0x60 in response to commands.
fn mouse_wait_read() {
    let mut status_port: Port<u8> = Port::new(0x64);
    for _ in 0..100_000 {
        unsafe {
            if (status_port.read() & 1) != 0 {
                return;
            }
        }
    }
}

/// Initializes the PS/2 Mouse hardware interface.
///
/// Sends command sequence to the Intel 8042 keyboard controller to enable
/// auxiliary device interrupts (IRQ 12), and sends instructions (Set Defaults,
/// Enable Data Reporting) directly to the mouse device.
/// Helper function to send a command to the auxiliary mouse device and wait for a 0xFA ACK.
/// Retries up to 3 times in case of transient bus contention.
fn mouse_write_cmd(cmd: u8) -> bool {
    let mut cmd_port: Port<u8> = Port::new(0x64);
    let mut data_port: Port<u8> = Port::new(0x60);
    
    for _ in 0..3 {
        // Wait for 8042 input buffer to be empty
        mouse_wait_write();
        unsafe {
            cmd_port.write(0xD4); // Tell 8042 that next data goes to mouse
        }
        
        // Write the command
        mouse_wait_write();
        unsafe {
            data_port.write(cmd);
        }
        
        // Wait for output buffer to be full
        mouse_wait_read();
        unsafe {
            if (cmd_port.read() & 1) != 0 {
                let ack = data_port.read();
                if ack == 0xFA {
                    return true;
                }
            }
        }
    }
    false
}

/// Initializes the PS/2 Mouse hardware interface.
///
/// Sends command sequence to the Intel 8042 keyboard controller to enable
/// auxiliary device interrupts (IRQ 12), and sends instructions (Set Defaults,
/// Enable Data Reporting) directly to the mouse device.
pub fn init_mouse() {
    let mut cmd_port: Port<u8> = Port::new(0x64);
    let mut data_port: Port<u8> = Port::new(0x60);

    unsafe {
        // 1. Drain any stale bytes from the 8042 output buffer first
        for _ in 0..20 {
            if (cmd_port.read() & 0x01) != 0 {
                let _ = data_port.read();
            } else {
                break;
            }
        }

        // 2. Enable auxiliary device port in 8042 controller
        mouse_wait_write();
        cmd_port.write(0xA8);

        // 3. Read current controller configuration byte
        mouse_wait_write();
        cmd_port.write(0x20);
        mouse_wait_read();
        let mut status = data_port.read();

        // 4. Set bit 1 (enable mouse interrupts) and clear bit 5 (enable mouse clock)
        status |= 0x02;
        status &= !0x20;

        // 5. Write updated configuration byte back to controller
        mouse_wait_write();
        cmd_port.write(0x60);
        mouse_wait_write();
        data_port.write(status);

        // 6. Send "Set Defaults" command (0xF6) to the mouse device
        let _ = mouse_write_cmd(0xF6);

        // 7. Send "Enable Data Reporting" command (0xF4) to the mouse device
        let _ = mouse_write_cmd(0xF4);
    }
}

/// Decodes a raw byte received via the IRQ 12 mouse interrupt vector.
///
/// Processes incoming data in groups of three bytes:
/// - Byte 1: Button states, signs and overflow flags.
/// - Byte 2: Delta X value (horizontal relative movement).
/// - Byte 3: Delta Y value (vertical relative movement).
///
/// Updates the global atomic variables `MOUSE_X`, `MOUSE_Y`, and button click states.
pub fn handle_mouse_interrupt(byte: u8) {
    unsafe {
        match MOUSE_CYCLE {
            0 => {
                // Bit 3 of the first byte must always be 1 to verify packet synchronization.
                // If it is 0, we discard this byte as out of sync.
                if (byte & 0x08) != 0 {
                    MOUSE_PACKET[0] = byte;
                    MOUSE_CYCLE = 1;
                }
            }
            1 => {
                MOUSE_PACKET[1] = byte;
                MOUSE_CYCLE = 2;
            }
            2 => {
                MOUSE_PACKET[2] = byte;
                MOUSE_CYCLE = 0;

                let flags = MOUSE_PACKET[0];
                // Decode signed 8-bit motion values directly using as i8 as i32
                let dx = MOUSE_PACKET[1] as i8 as i32;
                let dy = MOUSE_PACKET[2] as i8 as i32;

                // Check button click flags
                let left = (flags & 0x01) != 0;
                let right = (flags & 0x02) != 0;
                MOUSE_LEFT_CLICKED.store(left, Ordering::Relaxed);
                MOUSE_RIGHT_CLICKED.store(right, Ordering::Relaxed);

                // Read current coordinate values
                let old_x = MOUSE_X.load(Ordering::Relaxed);
                let old_y = MOUSE_Y.load(Ordering::Relaxed);

                // PS/2 mouse coordinate direction mapping:
                // Relative dx corresponds to positive horizontal steps.
                // Relative dy corresponds to positive vertical steps pointing UPwards.
                // Screen coordinates y axis points DOWNwards.
                let mut new_x = old_x + dx;
                let mut new_y = old_y - dy;

                // Clamp mouse cursor within display boundary
                if new_x < 0 {
                    new_x = 0;
                }
                if new_x >= SCREEN_WIDTH {
                    new_x = SCREEN_WIDTH - 1;
                }
                if new_y < 0 {
                    new_y = 0;
                }
                if new_y >= SCREEN_HEIGHT {
                    new_y = SCREEN_HEIGHT - 1;
                }

                MOUSE_X.store(new_x, Ordering::Relaxed);
                MOUSE_Y.store(new_y, Ordering::Relaxed);

                let event = usermode_x86::syscall::InputEvent {
                    event_type: 2, // Mouse
                    keyboard_key: 0,
                    mouse_x: new_x,
                    mouse_y: new_y,
                    mouse_left_clicked: if left { 1 } else { 0 },
                    mouse_right_clicked: if right { 1 } else { 0 },
                };
                crate::interrupts::push_input_event(event);
            }
            _ => {
                MOUSE_CYCLE = 0;
            }
        }
    }
}
