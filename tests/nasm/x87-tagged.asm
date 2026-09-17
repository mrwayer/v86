; The x87 forms that generated code keeps as a tagged double: the exchange,
; the constants, the sign changes, a register-to-register store, a square
; root, the compares, and the integer loads and stores under each rounding
; mode the control word names. Every value here is exact in a double, so the
; result does not depend on the precision the arithmetic runs at. Every stored
; status word drops C1, PE, OE and DE, the bits the harness does not compare
; either, so the fixture pins the bits the emulator implements and no other.

global _start

%include "header.inc"

    sub esp, 128

    mov dword [esp], 0x3fc00000           ; 1.5
    mov dword [esp+4], 0x40100000         ; 2.25
    mov dword [esp+8], 0x3fc80000         ; 1.5625
    mov dword [esp+12], 0xc0200000        ; -2.5
    mov dword [esp+16], 0x40200000        ; 2.5
    mov dword [esp+108], 0x47000000       ; 32768.0
    mov dword [esp+112], -1234
    mov dword [esp+116], 1

    ; an exchange, then arithmetic on the registers it swapped
    fld dword [esp]                       ; 1.5
    fld dword [esp+4]                     ; st0 2.25, st1 1.5
    fxch st1                              ; st0 1.5,  st1 2.25
    fdivp st1, st0                        ; 2.25 / 1.5
    fstp dword [esp+32]

    ; a register-to-register store, and the copy used as an operand
    fld dword [esp]                       ; 1.5
    fld dword [esp+4]                     ; st0 2.25, st1 1.5
    fst st1                               ; st1 = 2.25
    fmulp st1, st0                        ; 2.25 * 2.25
    fstp dword [esp+36]

    fld dword [esp]                       ; 1.5
    fld dword [esp+4]                     ; st0 2.25, st1 1.5
    fstp st1                              ; st1 = 2.25, then popped
    fstp dword [esp+124]

    ; the constants
    fld1
    fldz
    faddp st1, st0
    fistp dword [esp+40]                  ; 1
    fldpi
    fstp dword [esp+44]                   ; pi in a single

    ; the sign changes, including a negative zero
    fld dword [esp+12]                    ; -2.5
    fchs
    fabs
    fchs
    fabs
    fstp dword [esp+48]                   ; 2.5
    fldz
    fchs
    fstp dword [esp+52]                   ; a negative zero

    ; a root that is exact
    fld dword [esp+8]                     ; 1.5625
    fsqrt
    fstp dword [esp+56]                   ; 1.25

    ; an integer loaded, added to, and stored again
    fild dword [esp+112]                  ; -1234
    fld1
    faddp st1, st0
    fistp dword [esp+60]                  ; -1233

    ; the compares against a register
    fld dword [esp+4]                     ; 2.25
    fld dword [esp]                       ; st0 1.5, st1 2.25
    xor eax, eax
    fcom st1
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+64], eax
    xor eax, eax
    fcomp st1
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+68], eax
    fld dword [esp]                       ; st0 1.5, st1 2.25
    xor eax, eax
    fucompp
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+72], eax

    ; the compares against memory, against an integer in memory, and the
    ; quiet form that pops one register
    fld dword [esp]                       ; 1.5
    xor eax, eax
    fcom dword [esp+4]
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+20], eax
    xor eax, eax
    ficom dword [esp+116]
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+24], eax
    fld dword [esp+4]                     ; st0 2.25, st1 1.5
    xor eax, eax
    fucomp st1
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+28], eax
    fstp st0

    ; an integer store under each of the four rounding modes
    finit
    fnstcw [esp+120]
    mov ax, [esp+120]
    and ax, 0xf3ff
    mov [esp+122], ax                     ; the mode to come back to

    fld dword [esp+16]                    ; 2.5
    fistp dword [esp+76]                  ; 2, nearest with ties to even

    mov ax, [esp+122]
    or ax, 0x0400
    mov [esp+120], ax
    fldcw [esp+120]                       ; down
    fld dword [esp+16]
    fistp dword [esp+80]                  ; 2
    fld dword [esp+12]
    fistp dword [esp+84]                  ; -3

    mov ax, [esp+122]
    or ax, 0x0800
    mov [esp+120], ax
    fldcw [esp+120]                       ; up
    fld dword [esp+16]
    fistp dword [esp+88]                  ; 3
    fld dword [esp+12]
    fistp dword [esp+92]                  ; -2

    mov ax, [esp+122]
    or ax, 0x0c00
    mov [esp+120], ax
    fldcw [esp+120]                       ; towards zero
    fld dword [esp+16]
    fistp dword [esp+96]                  ; 2
    fld dword [esp+12]
    fistp dword [esp+100]                 ; -2

    mov ax, [esp+122]
    mov [esp+120], ax
    fldcw [esp+120]                       ; back to nearest

    ; a half rather than a word, and a value no half holds
    fld dword [esp+16]                    ; 2.5
    fistp word [esp+104]                  ; 2
    fld dword [esp+108]                   ; 32768.0
    fistp word [esp+106]                  ; the half's most negative value

%include "footer.inc"
