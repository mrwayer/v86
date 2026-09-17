; A fault in the middle of a block, after inline forms moved the stack top
; and the empty mask: the x87 state the fault leaves behind is what the
; fixture compares, so the top must have reached memory on the way out.

global _start

%include "header.inc"

    sub esp, 32

    mov dword [esp], 0x3fc00000           ; 1.5
    mov dword [esp+4], 0x40100000         ; 2.25

    fld dword [esp]                       ; 1.5, top 7
    fld dword [esp+4]                     ; 2.25, top 6
    fmul st0, st1                         ; 3.375
    ud2

%include "footer.inc"
