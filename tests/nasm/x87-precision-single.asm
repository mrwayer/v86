; Arithmetic with the control word's precision field at 24 bits: fadd, fsub,
; fmul, fdiv and fsqrt round their result to a single's mantissa, in each of
; the register and memory encodings, while the exponent keeps the format's
; range -- a result far above or below what a single holds is rounded to 24
; bits and stays finite. Every result is stored as a double, which holds a
; 24-bit mantissa exactly, so the memory compare is exact; the control word
; is put back before the end.

global _start

%include "header.inc"

    sub esp, 192

    mov dword [esp], 0x00000000           ; 1.0
    mov dword [esp+4], 0x3ff00000
    mov dword [esp+8], 0x00000000         ; 2^-30
    mov dword [esp+12], 0x3e100000
    mov dword [esp+16], 0x00000000        ; 3.0
    mov dword [esp+20], 0x40080000
    mov dword [esp+24], 0x08000000        ; 1 + 2^-25
    mov dword [esp+28], 0x3ff00000
    mov dword [esp+32], 0x00400000        ; (1 + 2^-30) * 2^300
    mov dword [esp+36], 0x52b00000
    mov dword [esp+40], 0x00000000        ; 2^-140
    mov dword [esp+44], 0x37300000
    mov dword [esp+48], 0x00400000        ; 1 + 2^-30
    mov dword [esp+52], 0x3ff00000
    mov dword [esp+56], 0x00000000        ; 0.0
    mov dword [esp+60], 0x00000000
    mov dword [esp+64], 0x40000000        ; 2.0 as a single

    fstcw [esp+72]
    mov ax, [esp+72]
    and ax, ~0x300
    mov [esp+76], ax
    fldcw [esp+76]

    ; against a double in memory
    fld qword [esp]                       ; 1.0
    fadd qword [esp+8]                    ; 1 + 2^-30 -> 1.0
    fstp qword [esp+80]
    fld qword [esp]
    fsub qword [esp+8]                    ; 1 - 2^-30 -> 1.0
    fstp qword [esp+88]
    fld qword [esp+16]                    ; 3.0
    fmul qword [esp+24]                   ; 3 + 3 * 2^-25 -> 3.0
    fstp qword [esp+96]
    fld qword [esp]
    fdiv qword [esp+16]                   ; 1/3 to 24 bits
    fstp qword [esp+104]

    ; against a single in memory
    fld qword [esp+48]                    ; 1 + 2^-30
    fmul dword [esp+64]                   ; 2 + 2^-29 -> 2.0
    fstp qword [esp+112]

    ; the three register encodings
    fld qword [esp+8]                     ; 2^-30
    fld qword [esp]                       ; st0 1.0, st1 2^-30
    fadd st0, st1                         ; 1.0
    fstp qword [esp+120]
    fld qword [esp]                       ; st0 1.0, st1 2^-30
    fsub st1, st0                         ; st1 = 2^-30 - 1 -> -1.0
    fstp st0
    fstp qword [esp+128]
    fld qword [esp+16]                    ; 3.0
    fld qword [esp+24]                    ; 1 + 2^-25
    fmulp st1, st0                        ; 3.0
    fstp qword [esp+136]

    ; the root
    fld qword [esp]                       ; 1.0
    fadd st0, st0                         ; 2.0
    fsqrt                                 ; sqrt 2 to 24 bits
    fstp qword [esp+144]

    ; results beyond a single's exponent: rounded to 24 bits, kept finite
    fld qword [esp+32]                    ; (1 + 2^-30) * 2^300
    fadd qword [esp+56]                   ; + 0.0 -> 2^300
    fstp qword [esp+152]
    fld qword [esp+40]                    ; 2^-140
    fmul qword [esp+48]                   ; * (1 + 2^-30) -> 2^-140
    fstp qword [esp+160]

    fldcw [esp+72]

%include "footer.inc"
