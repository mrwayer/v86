global _start

section .data

%include "header.inc"

    ; every width a store has, the read-modify-write forms, a push and a
    ; call, all into the stack the fixture compares
    mov eax, 0x8badf00d
    mov [esp], al
    mov [esp+2], ax
    mov [esp+4], eax
    fld1
    fstp qword [esp+8]
    movups xmm0, [esp]
    movups [esp+16], xmm0
    add dword [esp+4], 0x11
    inc byte [esp+1]
    xor word [esp+2], 0x55aa
    add dword [esp+20], eax
    xchg eax, [esp+24]
    mov ebx, 1
    xadd [esp+28], ebx
    push eax
    push ebx
    call callee
    push 0x1234
    pop ecx

%include "footer.inc"

callee:
    ret
