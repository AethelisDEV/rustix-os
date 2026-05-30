# AE Rustanium: Feature & Architecture Registry

This registry tracks all newly implemented features, system-level enhancements, and low-level architectural decisions to maintain consistency and AI-to-AI continuity.

---

## 🛠️ Low-Level Kernel Enhancements (`kernel-x86`)

### 1. Bare-Metal CPU Yield Bugfix (CPU Hang Prevention)
* **Date**: May 29, 2026
* **Description**: Replaced the raw assembly `hlt` instruction inside bare-metal target `Cpu::halt()` with `core::hint::spin_loop()`.
* **Rationale**: Because `kernel-x86` operates in polling mode and has interrupts disabled by default on boot, calling `hlt` put the CPU to sleep permanently. Replacing it with `spin_loop` (which compiles to a `pause` instruction) ensures the polling loops for the PS/2 keyboard and serial COM1 port continue executing smoothly.
* **Location**: `kernel-core/src/hal.rs`

### 2. SSE & FPU Hardware Activation on Boot
* **Date**: May 29, 2026
* **Description**: Added `enable_sse()` to initialize CR0 and CR4 control registers.
  * Clears `Cr0Flags::EMULATE_COPROCESSOR`.
  * Sets `Cr0Flags::MONITOR_COPROCESSOR`.
  * Sets `Cr4Flags::OSFXSR` (enabling FXSAVE/FXRSTOR).
  * Sets `Cr4Flags::OSXMMEXCPT_ENABLE` (enabling SIMD exceptions).
* **Rationale**: Prevent `Invalid Opcode (#UD)` exceptions and silent triple-faults when the compiler generates SSE/floating-point operations (e.g. for bulk memory clears or copies in `.bss`).
* **Location**: `kernel-x86/src/main.rs`

### 3. Interrupt Descriptor Table (IDT) & Exception Handling
* **Date**: May 29, 2026
* **Description**: Implemented a static `InterruptDescriptorTable` (IDT) loaded on boot.
* **Registered Handlers**:
  * **Breakpoint Exception**: Prints CPU stack frame to COM1.
  * **Double Fault Exception**: Triggers structured kernel panic.
  * **Page Fault Exception**: Intercepts illegal memory access, prints the faulting address from `CR2`, logs registers, and halts CPU safely.
* **Location**: `kernel-x86/src/main.rs` (Refactored and moved to `kernel-x86/src/interrupts.rs`)

### 4. Interrupt-Driven Asynchronous Kernel Architecture (PIC, PIT Timer, Asynchronous Keyboard)
* **Date**: May 29, 2026
* **Description**: Replaced the entire synchronous polling-based I/O loop with a highly advanced, fully asynchronous, interrupt-driven architecture.
* **Key Components**:
  * **8259 Chained PICs Controller**: Configured via direct port writes to map hardware IRQs (IRQ 0-15) to custom CPU vector offsets (32-47).
  * **100 Hz PIT (Programmable Interval Timer - IRQ 0)**: Configured using rate generator mode to tick asynchronously at precisely 100 Hz, advancing kernel `ticks` in the background.
  * **Asynchronous Keyboard Sentry (IRQ 1)**: Intercepts PS/2 key strikes, decodes them on the fly, and registers them in a thread-safe static buffer (`KEYBOARD_BUFFER`).
  * **Idle Sleep (HLT Loop)**: The main execution loop is now entirely passive, utilizing `x86_64::instructions::hlt()` to sleep the CPU. It wakes up exclusively upon hardware interrupts, processes queued actions, and sleeps again—perfectly mimicking the idle behavior of mature kernels like Linux.
* **Location**: `kernel-x86/src/interrupts.rs` (New module) and `kernel-x86/src/main.rs` (Integrated loop)

### 5. Cooperative Multitasking & Assembly Context Switching
* **Date**: May 29, 2026
* **Description**: Implemented a low-level cooperative scheduler allowing threads to run in parallel on independent 8 KB stacks.
* **Key Components**:
  * **Thread Control Block (TCB)**: Manages dynamic 8 KB stack spaces, thread IDs, execution status, and saved stack pointers.
  * **Assembly Context Switcher (`switch_context`)**: A raw, inline-assembly routine that pushes callee-preserved registers (rbp, rbx, r12, r13, r14, r15) to the active stack, saves the stack pointer (rsp), loads the new thread's stack pointer, and pops the registers, returning to the new execution stream.
  * **Round-Robin Scheduling**: Swaps thread execution sequentially during yields.
  * **Background Micro-tasks**: Spawned `thread_scrubber` (memory sweeping daemon) and `thread_diagnostics` (logging engine) executing in parallel loops cooperatively.
* **Location**: `kernel-x86/src/scheduler.rs` (New module) and `kernel-x86/src/main.rs` (Spawned threads & yields)

### 6. UEFI & BIOS Dual-Boot & GOP Framebuffer Graphics
* **Date**: May 29, 2026
* **Description**: Migrated from legacy BIOS boot (`bootloader` v0.9) to modern **UEFI & BIOS Dual-Boot** architecture (`bootloader_api` v0.11). Replaced the fragile `0xB8000` VGA text mode writes with direct graphics rendering on the UEFI **Graphics Output Protocol (GOP)** linear framebuffer.
* **Key Components**:
  * **UEFI Graphics Engine (`framebuffer.rs`)**: Encapsulates pixel-level drawing with auto-detection of hardware RGB/BGR layout, drawing color/gradient panels, status blocks, and interactive text.
  * **Embedded 8x8 Bitmap Font**: Embedded a lightweight 8x8 monospace bitmap font to print console and shell text directly to GOP.
  * **Smart Visual Redraw Optimization**: Implemented a state-differential check rendering the screen *only* when ticks update or line buffer lengths change, eliminating CPU draw spikes.
* **Location**: `kernel-x86/src/framebuffer.rs` (New module) and `kernel-x86/src/main.rs` (Bootloader entry & rendering)

### 7. Physical Hardware Interrupt Safety (Direct I/O Port Polling)
* **Date**: May 29, 2026
* **Description**: Deactivated external hardware interrupts (`cli` mode) by commenting out `x86_64::instructions::interrupts::enable();`. 
* **Rationale**: Bypasses legacy PIC/APIC motherboard routing conflicts and flaky USB emulation layers on modern physical UEFI machines. Since we already employ a highly responsive cooperative polling routine for both the PS/2 keyboard (ports `0x60` and `0x64`) and COM1 UART serial ports directly in our main execution loop, external hardware interrupts are completely unnecessary. synchronous CPU exceptions (GPF, Page Faults) still trigger perfectly.
* **Location**: `kernel-x86/src/main.rs`

### 8. Dynamic Memory Reclamation & LockedHeap Integration
* **Date**: May 29, 2026
* **Description**: Transitioned the global kernel allocator from the leaky bump allocator to `linked_list_allocator::LockedHeap` with a 1 MB heap buffer.
* **Rationale**: A bump allocator never reclaims memory upon deallocation, causing the small 256 KB heap to be completely exhausted after 154 ticks of continuous telemetry allocations in `core.tick()`. `LockedHeap` dynamically reclaims deallocated memory, allowing the system to tick indefinitely (tested past 500+ sweeps) without memory leaks.
* **Location**: `kernel-x86/Cargo.toml` and `kernel-x86/src/main.rs`

### 9. Unified Visual Panic Screen & GraphicsWriter Stream
* **Date**: May 29, 2026
* **Description**: Overhauled exception and panic handlers to forcefully unlock the global static `GRAPHICS` spinlock and render detailed error stack traces visually.
* **Key Components**:
  * **GraphicsWriter (`framebuffer.rs`)**: A custom formatting stream conforming to the standard `core::fmt::Write` trait, allowing heap-free directly-formatted string writes onto the graphics framebuffer.
  * **Unified Panic Screen**: If a kernel panic, double fault, page fault, GPF, invalid opcode, or divide-by-zero occurs, the system renders a bright red diagnostic console box on screen, providing the exact file, line, and stack trace to aid native hardware debugging.
* **Location**: `kernel-x86/src/framebuffer.rs`, `kernel-x86/src/interrupts.rs`, and `kernel-x86/src/main.rs`

### 10. Direct Serial Keyboard Echo, Prompt Visibility, & Turkish Layout Support
* **Date**: May 30, 2026
* **Description**: 
  * Diverted character/backspace input echoing from the standard `print!` macro (which appends directly to `TTY_LOGS` line-by-line) to write directly to `SERIAL_WRITER`. When the user hits Enter, the full prompt and command line are formatted and appended to `TTY_LOGS` as a single unified line.
  * Added explicit calls to draw the command prompt during kernel boot initialization, and set `last_rendered_len = 9999` to trigger immediate prompt rendering on the first loop cycle.
  * Modified `_print` to suppress background diagnostics spam (`[THREAD 1]` and `[THREAD 2]` logs) from being output to the serial console, keeping the active input line and prompt clean and always visible.
  * Added a `loadkeys` command (e.g. `loadkeys trq`) to switch between US and Turkish Q (TRQ) keyboard layout scancode decoding tables dynamically. Maps Turkish characters to standard ASCII equivalents so they render beautifully with embedded fonts and compile cleanly with CLI inputs.
* **Rationale**: 
  * Bypasses the issue where every typed character or backspace was appended to `TTY_LOGS` as a separate entry, causing the TTY console screen scrollback to shift upward with every single keystroke. Now, live typing only echoes to the serial port and updates the dedicated, static interactive prompt at the bottom of the screen, preserving the scrollback history cleanliness.
  * Ensures that the `rustanium:/>` shell prompt is immediately visible when the system boots up and when switching between modes, rather than only appearing after a character is typed.
  * Prevents background thread logs from constantly interrupting and scrambling the active input line on the serial console, so the prompt `rustanium:/>` is never pushed out of view.
  * Enables Turkish Q keyboard users to type on native console layouts while automatically mapping non-ASCII chars to ASCII look-alikes to bypass font index limits (which made characters completely invisible).
* **Location**: `kernel-x86/src/keyboard.rs` and `kernel-x86/src/shell.rs`

### 11. Monolithic main.rs Modular Split (God Object Refactoring)
* **Date**: May 30, 2026
* **Description**: Fully refactored and split the 1400+ line `main.rs` monolithic entry point into three highly cohesive, single-responsibility sub-modules:
  * **`logger.rs`**: Handles print macros (`print!`, `println!`), serial writer bindings (`SERIAL_WRITER`), TTY log buffers, and telemetry suppression.
  * **`keyboard.rs`**: Encapsulates layouts (`Us`, `Trq`), shift status flags, scancode tables, hardware polling (`poll_keyboard`), and serial polling (`poll_serial`).
  * **`shell.rs`**: Exposes the interactive microkernel parser (`handle_command`), VFS relative path resolver, directory tree traversal, and CLI commands.
* **Rationale**: Adheres to strict Single Responsibility Principles (SRP) and the file length limit of 800 lines specified in `AI_GUIDELINES.md`. This slims down the entry point, isolating low-level CPU bootstrapping from CLI logic and input decoding, dramatically improving system maintainability.
* **Location**: `kernel-x86/src/main.rs`, `kernel-x86/src/logger.rs`, `kernel-x86/src/keyboard.rs`, and `kernel-x86/src/shell.rs`

---

## 📊 Visual Telemetry & Interface Enhancements

### 1. TMR stability & ALU Voter Diagnostics panel
* **Date**: May 29, 2026
* **Description**: Added a real-time TMR Diagnostics panel directly in the visual aerospace flight visualizer.
* **Features**:
  * Computes **TMR Voter Stability Index** dynamically based on total critical cycles and ALU voter interceptions.
  * Draws an ANSI-colored retro text bar graph representing stability percentage and stability state (Green / Yellow / Red).
  * Tracks and displays precise **ALU Fault Rate** percentages.
* **Location**: `simulation-dashboard/src/main.rs`

---

## 📦 Workspace Configuration Adjustments

### 1. Workspace Build Default-Members Exclusion
* **Date**: May 29, 2026
* **Description**: Added `default-members` configuration to the workspace Cargo.toml to exclude `kernel-x86`.
* **Rationale**: Prevents `duplicate lang item panic_impl` compiler errors when running cargo commands at the workspace root without target variables. Ensures host-side simulation runs seamlessly, while target-specific builds are cleanly isolated.
* **Location**: `Cargo.toml` (Workspace Root)

### 2. Host-Side Runner Crate Integration
* **Date**: May 29, 2026
* **Description**: Integrated a new host-side compilation package `runner` to orchestrate kernel building, image generation, and QEMU execution.
* **Rationale**: Automates `cargo +nightly` bare-metal builds and calls `bootloader` library routines programmatically to output BOTH a flashable modern GPT `uefi.img` and a legacy MBR `bios.img` directly, then boots QEMU with COM1 mapped directly to terminal stdio.
* **Location**: `runner/Cargo.toml` and `runner/src/main.rs`
