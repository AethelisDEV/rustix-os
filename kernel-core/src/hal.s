.intel_syntax noprefix
.global hal_write_byte
hal_write_byte:
    mov dx, 0x3F8
    mov al, dil
    out dx, al
    ret
