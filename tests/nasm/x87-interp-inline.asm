; The forms the interpreter runs on tagged registers the way generated code
; does: the loads from memory, the arithmetic against memory and against a
; register in each of its three encodings, the stores, and the compares. Every
; value is exact in a double, so the result does not depend on the precision
; the arithmetic runs at, and the same file checks the interpreter and the
; compiler against one hardware fixture. The last two cases are the ones the
; arm must decline: a value no double holds, and an empty register.

global _start

%include "header.inc"

    sub esp, 256

    mov dword [esp], 0x3fc00000           ; 1.5
    mov dword [esp+4], 0x40100000         ; 2.25
    mov dword [esp+8], 0x40400000         ; 3.0
    mov dword [esp+12], 0x7fc00000        ; a quiet NaN

    mov dword [esp+16], 0x00000000        ; 2.25 as a double
    mov dword [esp+20], 0x40020000
    mov dword [esp+24], 0x00000000        ; 1.5 as a double
    mov dword [esp+28], 0x3ff80000

    ; 1.5 in the 80-bit format with the lowest mantissa bit set: no double
    ; holds it
    mov dword [esp+32], 0x00000001
    mov dword [esp+36], 0xc0000000
    mov word [esp+40], 0x3fff

    ; arithmetic against a single in memory
    fld dword [esp]                       ; 1.5
    fadd dword [esp+4]                    ; 3.75
    fstp dword [esp+64]
    fld dword [esp]
    fsub dword [esp+4]                    ; -0.75
    fstp dword [esp+68]
    fld dword [esp]
    fsubr dword [esp+4]                   ; 0.75
    fstp dword [esp+72]
    fld dword [esp+8]                     ; 3.0
    fdiv dword [esp]                      ; 2.0
    fstp dword [esp+76]
    fld dword [esp]                       ; 1.5
    fdivr dword [esp+8]                   ; 2.0
    fstp dword [esp+80]

    ; arithmetic against a double in memory, stored as a double
    fld qword [esp+16]                    ; 2.25
    fmul qword [esp+24]                   ; 3.375
    fst qword [esp+88]
    fstp qword [esp+96]
    fld qword [esp+24]                    ; 1.5
    fadd qword [esp+16]                   ; 3.75
    fstp dword [esp+84]

    ; the three register encodings: st(0) as the target, st(i) as the target,
    ; and st(i) as the target with a pop
    fld dword [esp+4]                     ; 2.25
    fld dword [esp]                       ; st0 1.5, st1 2.25
    fadd st0, st1                         ; st0 3.75
    fmul st1, st0                         ; st1 8.4375
    fsubp st1, st0                        ; st0 4.6875
    fstp dword [esp+104]
    fld dword [esp+8]                     ; 3.0
    fld dword [esp]                       ; st0 1.5, st1 3.0
    fsubrp st1, st0                       ; -1.5
    fstp dword [esp+108]
    fld dword [esp]                       ; 1.5
    fld dword [esp+8]                     ; st0 3.0, st1 1.5
    fdivp st1, st0                        ; 0.5
    fstp dword [esp+112]
    fld dword [esp+8]                     ; 3.0
    fld dword [esp]                       ; st0 1.5, st1 3.0
    fdivrp st1, st0                       ; 0.5
    fstp dword [esp+116]

    ; the compares, against memory and against a register
    fld dword [esp+4]                     ; 2.25
    fld dword [esp]                       ; st0 1.5, st1 2.25
    xor eax, eax
    fcom dword [esp+8]
    fnstsw ax
    mov [esp+120], eax
    xor eax, eax
    fcom qword [esp+24]
    fnstsw ax
    mov [esp+124], eax
    xor eax, eax
    fcom st1
    fnstsw ax
    mov [esp+128], eax
    xor eax, eax
    fucom st1
    fnstsw ax
    mov [esp+132], eax
    xor eax, eax
    fcomp dword [esp]                     ; equal, then popped
    fnstsw ax
    mov [esp+136], eax
    fld dword [esp+8]                     ; st0 3.0, st1 2.25
    xor eax, eax
    fcomp st1
    fnstsw ax
    mov [esp+140], eax
    fld dword [esp+8]                     ; st0 3.0, st1 2.25
    xor eax, eax
    fucomp st1
    fnstsw ax
    mov [esp+144], eax
    fld dword [esp+4]                     ; st0 2.25, st1 2.25
    xor eax, eax
    fcompp
    fnstsw ax
    mov [esp+148], eax
    fld dword [esp]
    fld dword [esp+4]
    xor eax, eax
    fucompp
    fnstsw ax
    mov [esp+152], eax
    fld dword [esp+4]
    fld dword [esp]
    xor eax, eax
    fcomp qword [esp+16]
    fnstsw ax
    mov [esp+156], eax
    fstp st0

    ; a NaN: unordered under the quiet compare, and an invalid operation
    ; under the signalling one
    fld1
    fld dword [esp+12]
    xor eax, eax
    fucom st1
    fnstsw ax
    mov [esp+160], eax
    xor eax, eax
    fcom st1
    fnstsw ax
    mov [esp+164], eax
    fstp st0
    fstp st0
    finit

    ; a value no double holds: the arithmetic is the library's, and the
    ; result narrowed to a single is the same either way
    fld tword [esp+32]
    fadd dword [esp+4]
    fstp dword [esp+168]

    ; an empty register: the fault the helper reports, and its result. The
    ; stack never went deeper than two here, so the slot st(1) names was
    ; never written and carries no tag from an earlier value -- generated
    ; code reads the tag alone and does not keep the empty bookkeeping.
    fld dword [esp]                       ; st0 1.5, st1 empty
    fadd st0, st1
    xor eax, eax
    fnstsw ax
    mov [esp+172], eax
    fstp st0

%include "footer.inc"
