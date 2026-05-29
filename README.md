# 🌟 AE Rustanium: Safe, Fault-Tolerant & Self-Healing Microkernel Simulation

[![Rust](https://img.shields.io/badge/rust-stable%20%2F%20nightly-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Safety](https://img.shields.io/badge/safety-Safe%20Core%20%2F%20Bare--metal%20Unsafe-success.svg)](#safety--architecture-principles)

**AE Rustanium** is a custom, microkernel-inspired operating system and simulation environment designed specifically to handle **hardware bit-flips, silent data corruption, and cosmic radiation**. It combines high-reliability **Safe Rust core abstractions** for host-side simulation with an **experimental bare-metal x86-64 target** that safely manages low-level hardware interactions.

Built for environments susceptible to Single Event Upsets (SEUs) such as aerospace, deep space missions, high-altitude aviation, or edge nodes lacking hardware ECC RAM, AE Rustanium dynamically turns standard virtual pages into adaptive self-healing structures.

---

## 🚀 Key Features

*   **🛡️ Safe Core / Bare-Metal Unsafe Boundaries**: The core flight simulation components (`memory-subsystem`, `scheduler`, `virtual-fs`, `kernel-core`) are built using 100% safe Rust to guarantee memory safety. Low-level bare-metal hardware components in the `kernel-x86` target leverage standard, strictly isolated `unsafe` blocks for GDT, interrupts, and direct hardware register access.
*   **💾 Software-Defined ECC Pages**: Implements a robust SECDED (Single Error Correction, Double Error Detection) Hamming Code encoder and decoder. Data is encoded on write and verified on read.
*   **🧹 Memory Scrubbing Daemon**: A background task sweeps physical memory page-by-page, correcting silent bit-flips (cosmic ray emulation) before they trigger application panics.
*   **☣️ Dynamic Page Quarantine & Hot-Swap**: If a physical memory frame experiences a severe, uncorrectable double-bit flip, the microkernel quarantines that frame, dynamically allocates a healthy one, and relocates active task memory transparently.
*   **🗳️ Triple Modular Redundancy (TMR)**: Protects critical calculations (e.g., flight control navigation) from register/ALU corruption by executing tasks in triplicate. An ALU voter uses 2-out-of-3 majority rule to repair register state on the fly.
*   **🗄️ Unix-Like Inode VFS**: A fully decoupled Virtual File System that writes, reads, and maps directories and files directly onto the ECC-protected virtual physical memory blocks.
*   **🛸 Retro Aerospace Visual Console**: A beautiful, terminal-based real-time telemetry dashboard styled using raw ANSI escape codes. Observe scrubber sweeps, memory grid status, TMR voters, and interactively inject radiation faults!

---

## 📐 Architecture & Workspace Structure

AE Rustanium is designed with strict module boundaries inside a Cargo workspace:

```
AE Rustanium/ (Workspace Root)
├── Cargo.toml
├── kernel-core/          # System bootstrapping, modules registration, & HAL
├── memory-subsystem/     # Software SECDED ECC, page frame allocator, & background scrubber
├── scheduler/            # Preemptive task dispatcher & TMR voting engine
├── virtual-fs/           # Inode-based virtual filesystem backed by virtual RAM
├── simulation-dashboard/ # Terminal GUI, telemetry dashboard, & fault injector
└── kernel-x86/           # Bare-metal x86-64 target wrapper using the bootloader crate
```

---

## 🎮 The Telemetry & Control Dashboard

When running the simulation, you are presented with a futuristic aerospace command console that operates in real time:

```
==============================================================================
||              AE RUSTANIUM OS - MODULAR SELF-HEALING MICROKERNEL          ||
||  [Bit-Flip Fault Tolerance] | [Active Fault Mitigation Flight Controller]  ||
==============================================================================

  PHYSICAL RAM PAGE GRID (8x8)               SYSTEM TELEMETRY DIAGNOSTICS
  -----------------------------              ---------------------------------
  [T] [ ] [ ] [ ] [ ] [ ] [ ] [ ]            System Clock Ticks     : 42
  [ ] [N] [ ] [ ] [ ] [ ] [ ] [ ]            Active Processes       : 3 (T: Tele, N: Navi, L: Life)
  [ ] [ ] [L] [ ] [ ] [ ] [ ] [ ]            Background Sweeps      : 5
  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]            Single-Bit ECC Repairs : 2 (Hamming SECDED)
  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]            Quarantined Pages (X)  : 1 (MMU Isolated)
  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]            Dynamic Relocations    : 1 (Self-Healing)
  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]            Redundant TMR Tasks    : 3
  [ ] [ ] [ ] [ ] [ ] [ ] [ ] [ ]            ALU Voter Corrections  : 1 (Triple Redundancy)

  Legend: [ ] Healthy | [ ] ECC Corrected | [X] Quarantined | T/N/L: Running Tasks
```

### Dashboard Actions:
1.  **[1] Advance Kernel Clock (Tick)**: Step the CPU clock, dispatch tasks, and trigger memory sweeps.
2.  **[2] Inject Random Single-Bit Flip**: Simulates a cosmic ray ionizing a memory cell. Watch the SECDED decoder automatically correct it during a read or the scrubber sweep it.
3.  **[3] Inject Random Double-Bit Flip**: Triggers severe data corruption. Watch the MMU page fault intercept this, quarantine the frame, allocate a new page, and migrate the task!
4.  **[4] Inject ALU Register Bit Flip**: Corrupts the registers of a critical TMR process. The voter engine intercepts the divergence, applies the majority rule, corrects the corrupt thread, and continues without downtime.
5.  **[5] Enable Autonomous Autopilot**: Ticks the system continuously with rich animations to watch self-healing dynamics in real time.
6.  **[6] Enter Unix Command Shell**: Drop into a command terminal operating directly inside the self-healing VFS.
7.  **[0] System Power Down**: Power off and halt the OS.

---

## 🐚 Decentralized Unix Terminal Shell

Enter action `[6]` from the control panel to access the virtual Unix shell. It operates inside a custom inode filesystem mapping files onto the self-healing RAM frames.

Available commands:
*   `ls [path]` — List directory entries (e.g., `ls /system`)
*   `cd [path]` — Traverses directory hierarchy (`cd /data/logs` or `cd ..`)
*   `mkdir <path>` — Create nested directories
*   `touch <path>` — Create empty virtual files
*   `cat <path>` — Outputs file contents with SECDED parity verification
*   `write <path> <text>` — Encodes text into SECDED words and writes to VFS
*   `rm [-rf] <path>` — Recursively delete files and folders
*   `hexdump <path>` — View raw bytes in Hex and ASCII
*   `hexdump -p <page_index>` — **Dump raw 13-bit SECDED physical memory registers** directly from virtual RAM frames!
*   `df` — View disk/memory capacity, free blocks, and a live health visualizer bar
*   `ps` — Monitor running microkernel processes and TMR protection status
*   `sudo <command>` — Run terminal command with administrator flight clearance logs
*   `exit` — Gracefully return to the main dashboard

---

## 🛠️ Build and Verification Guide

### Prerequisites
*   [Rust Stable Toolchain](https://rustup.rs/) (to run the simulation & dashboard)
*   [Rust Nightly Toolchain](https://rustup.rs/) (required *only* for the experimental bare-metal `kernel-x86` target)

### Running the Interactive Simulation
Compile and boot the interactive visual flight dashboard instantly using standard Rust:
```bash
cargo run --package simulation-dashboard
```

### Running the Workspace Test Suite
Validate the SECDED Hamming math, memory allocation bounds, TMR majority voters, and VFS inode trees:
```bash
cargo test --workspace
```

### Zero-Warning Verification
Check the workspace using Clippy under strict rules:
```bash
cargo clippy --workspace -- -D warnings
```

---

## 🛰️ UEFI & BIOS Dual-Boot Bare-Metal Target (`kernel-x86`)

The `kernel-x86` crate is a fully-fledged bare-metal x86-64 operating system target. Using the `bootloader_api` (v0.11) framework, it supports booting on both legacy BIOS systems and modern UEFI flight computers (such as Ryzen 7000/9000 AM5 series PCs).

### 🖥️ High-Performance UEFI GOP Framebuffer Graphics
Instead of relying on outdated `0xB8000` VGA text mode (which crashes on modern UEFI systems), AE Rustanium dynamically binds to the UEFI **Graphics Output Protocol (GOP)** linear framebuffer. It renders an extremely premium, dark-themed visual dashboard containing:
*   **Active Thread Status Cards**: Live rendering of Thread 1 (Memory Scrubber sweep) and Thread 2 (Telemetry engine).
*   **System Diagnostics metrics**: Live ticks and voter health percentage.
*   **Interactive Event Terminal**: Echoes PS/2 keyboard strikes in real-time with pixel-perfect font scaling using an embedded 8x8 bitmap font.
*   **Dynamic Progress Indicators**: Heartbeat pulse representing the active scheduler time slice.

![AE Rustanium UEFI Graphics Console](assets/uefi_console.png)

### 🔌 Physical Hardware Optimization & Safety
For native booting on modern flight hardware (such as AMD Ryzen platforms), several critical architectural enhancements have been implemented:
1.  **Direct I/O Port Polling (Interrupt Safety)**: Modern UEFI systems ignore or mask legacy 8259 PIC interrupts, and BIOS/UEFI USB keyboard emulation can cause unhandled interrupt conflicts. To guarantee absolute stability, CPU hardware interrupts are kept completely disabled (`cli`). The PS/2 keyboard is read directly from ports `0x60` and `0x64` via a robust polling driver integrated into the main loop, bypassing legacy interrupts entirely.
2.  **Unified Visual Panic Screen (Red Screen of Death)**: When a kernel panic or CPU exception (GPF, Page Fault, Stack Segment Fault, Divide by Zero) occurs on physical hardware, there is no serial debugger attached. We implemented a custom `GraphicsWriter` that formats text directly to the UEFI GOP framebuffer. If a crash occurs, the handler forcefully releases graphics locks and displays the crash dump visually in a bright red diagnostic console on the monitor instead of silently freezing.
3.  **LockedHeap Dynamic Memory Allocation**: Standard bare-metal OS bump allocators leak memory upon deallocation, causing loop allocations in a continuous tick loop to run out of memory. We integrated the standard `linked_list_allocator::LockedHeap` with a 1 MB static heap buffer, enabling full memory reclamation and ensuring indefinite, leak-free continuous runtime on real hardware.

---

### 🚀 Running the Kernel in QEMU (Zero-Setup Emulation)
We have implemented an automated workspace builder and runner. You do not need to install `bootimage` or any external tools!

1. Make sure you have [QEMU](https://www.qemu.org/) installed at `C:\Program Files\qemu\qemu-system-x86_64.exe`.
2. Run the automated host runner from your terminal:
   ```bash
   cargo run --package runner
   ```
This command programmatically:
1. Compiles `kernel-x86` for `x86_64-unknown-none`.
2. Generates a legacy BIOS boot disk image at `target/x86_64-unknown-none/debug/bios.img`.
3. Generates a modern GPT UEFI boot disk image at `target/x86_64-unknown-none/debug/uefi.img`.
4. Spawns QEMU using the BIOS image with COM1 serial port output piped directly to your active console in real time!

---

### 🔌 Booting on a Real UEFI PC (e.g. Ryzen 7500F / RTX 3070 Ti)
To boot this kernel on your physical computer:
1. Locate the generated **`uefi.img`** at `target/x86_64-unknown-none/debug/uefi.img` (or `release/uefi.img` if compiled with `cargo run --package runner -- --release`).
2. Insert a USB flash drive.
3. Open a raw writer tool like **Rufus**:
   *   Select your USB Drive.
   *   Choose "Disk or ISO Image" and select `uefi.img`.
   *   Ensure you write it in **DD Image Mode** (this writes the raw GPT partition containing the bootloader `/EFI/BOOT/BOOTX64.EFI` directly).
4. Restart your PC, enter UEFI/BIOS settings, select the USB drive as the primary boot device, and watch AE Rustanium boot directly on real hardware!

---

## 📜 License

This project is licensed under the Apache License, Version 2.0. See the [LICENSE](LICENSE) and [NOTICE](NOTICE) files for details.
