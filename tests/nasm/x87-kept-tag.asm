; The forms whose helper used to leave a register in a form generated code
; could not read inline: an 80-bit load of a value a double holds, a word
; integer load, a round to an integer, and a scale. Each is followed by
; arithmetic on what it wrote, which is where the cost of losing the form was
; paid. Every value here is exact in a double, so the result does not depend on
; the precision the arithmetic runs at -- except the last, which is a value no
; double holds and whose arithmetic must therefore stay the library's.

global _start

%include "header.inc"

    sub esp, 256

    mov dword [esp], 0x3fc00000           ; 1.5
    mov dword [esp+4], 0x40100000         ; 2.25
    mov dword [esp+8], 0x40f00000         ; 7.5

    ; 1.5 as the 80-bit format: a double holds it exactly
    mov dword [esp+16], 0x00000000
    mov dword [esp+20], 0xc0000000
    mov word [esp+24], 0x3fff

    ; the same, with the lowest mantissa bit set: no double holds it
    mov dword [esp+32], 0x00000001
    mov dword [esp+36], 0xc0000000
    mov word [esp+40], 0x3fff

    mov word [esp+48], 1234
    mov word [esp+50], -3

    ; an 80-bit load, then arithmetic on what it loaded
    fld tword [esp+16]                    ; 1.5
    fmul dword [esp+4]                    ; 3.375
    fstp dword [esp+128]

    ; a word integer load, then arithmetic on what it loaded
    fild word [esp+48]                    ; 1234
    fmul dword [esp]                      ; 1851
    fstp dword [esp+132]
    fild word [esp+50]                    ; -3
    fadd dword [esp+4]                    ; -0.75
    fstp dword [esp+136]

    ; a round to an integer, then arithmetic on the rounded value
    fld dword [esp+8]                     ; 7.5
    frndint                               ; 8 -- to nearest, ties to even
    fmul dword [esp]                      ; 12
    fstp dword [esp+140]

    ; a scale, then arithmetic on the scaled value
    fld dword [esp+4]                     ; 2.25
    fld1
    fxch st1
    fscale                                ; 4.5
    fmul dword [esp]                      ; 6.75
    fstp dword [esp+144]
    fstp st0

    ; a value no double holds: the arithmetic stays the library's, and the
    ; result narrowed to a single is the same either way
    fld tword [esp+32]
    fmul dword [esp+4]
    fstp dword [esp+148]

%include "footer.inc"
