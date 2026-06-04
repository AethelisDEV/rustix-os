/* ============================================================================
 * AE Rustanium - Ring 3 usermode-desktop Linker Script
 * ============================================================================
 * Places the user-space desktop binary at a fixed virtual address 0x400000.
 * This allows the kernel to map pages at this exact address, ensuring all
 * absolute references (GOT, static data, function pointers) resolve correctly.
 *
 * The .text.start section is placed FIRST so that _start is at byte 0 of the
 * flat binary produced by llvm-objcopy -O binary.
 * ============================================================================ */

ENTRY(_start)

SECTIONS
{
    /* Fixed user-space base virtual address (4 MB) */
    . = 0x400000;

    /* Code segment - entry point first for flat binary compatibility */
    .text ALIGN(16) : {
        KEEP(*(.text.start))
        *(.text .text.*)
    }

    /* Read-only data */
    .rodata ALIGN(16) : {
        *(.rodata .rodata.*)
    }

    /* Exception handling frames (kept for stack unwinding) */
    .eh_frame_hdr : { *(.eh_frame_hdr) }
    .eh_frame     : { *(.eh_frame) }

    /* Data with relocated pointers (resolved at link time for static binary) */
    .data.rel.ro ALIGN(16) : {
        *(.data.rel.ro .data.rel.ro.*)
    }

    /* Global Offset Table (pre-filled with absolute addresses in static mode) */
    .got ALIGN(8) : {
        *(.got .got.*)
    }

    /* Initialized read-write data */
    .data ALIGN(16) : {
        *(.data .data.*)
    }

    /* Zero-initialized data (BSS) - not included in flat binary, zeroed at load */
    .bss ALIGN(4096) : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /* Discard unnecessary dynamic linking and debug metadata sections */
    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.dynamic)
        *(.dynsym)
        *(.dynstr)
        *(.gnu.hash)
        *(.hash)
        *(.rela.dyn)
        *(.rela.plt)
        *(.relro_padding)
    }
}
