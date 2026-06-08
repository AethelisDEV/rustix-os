.intel_syntax noprefix

.global hal_write_byte
hal_write_byte:
    mov dx, 0x3F8
    mov al, dil
    out dx, al
    ret

.global pci_read_config_dword
pci_read_config_dword:
    mov dx, 0xCF8
    mov eax, edi
    out dx, eax
    mov dx, 0xCFC
    in eax, dx
    ret

.global pci_write_config_dword
pci_write_config_dword:
    mov dx, 0xCF8
    mov eax, edi
    out dx, eax
    mov dx, 0xCFC
    mov eax, esi
    out dx, eax
    ret
