; fscale beyond what a double holds: a scale past 2^1023 or below 2^-1074
; stays a finite power of two in the 80-bit format, and a mantissa with all
; sixty-four bits set comes through untouched under single precision control,
; since the instruction is not one the control word rounds. The results are
; stored in the 80-bit format, so the memory compare is exact.

global _start

%include "header.inc"

    sub esp, 96

    mov dword [esp], 1100
    fild dword [esp]                      ; st0 1100
    fld1                                  ; st0 1, st1 1100
    fscale                                ; st0 2^1100
    fstp tword [esp+16]
    fstp st0

    mov dword [esp], -1100
    fild dword [esp]
    fld1
    fscale                                ; 2^-1100
    fstp tword [esp+32]
    fstp st0

    ; 1.5 scaled by the integer part of 2.75
    mov dword [esp], 0x40300000           ; 2.75
    mov dword [esp+4], 0x3fc00000         ; 1.5
    fld dword [esp]
    fld dword [esp+4]
    fscale                                ; 6.0
    fstp qword [esp+48]
    fstp st0

    ; every mantissa bit set, scaled by 5 with the control word at single
    ; precision: the mantissa is kept whole
    fstcw [esp+8]
    and word [esp+8], ~0x300
    fldcw [esp+8]
    mov dword [esp+64], 0xffffffff
    mov dword [esp+68], 0xffffffff
    mov word [esp+72], 0x3fff
    mov dword [esp], 5
    fild dword [esp]
    fld tword [esp+64]
    fscale                                ; the same mantissa, exponent 0x4004
    fstp tword [esp+80]
    fstp st0

%include "footer.inc"
