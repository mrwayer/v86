; Precision control at 24 bits governs FADD/FSUB/FMUL/FDIV/FSQRT and nothing
; else: FPREM and FYL2X compute their intermediate multiply, divide and
; subtract at the full 64-bit significand regardless of the control word, so
; their result must be the one hardware computes exactly, not the one a
; 24-bit-narrowed intermediate would give. FIADD -- unlike FADD st, st(i),
; which an inline arm may take -- is always the library path in the
; interpreter, so it is what a regression in that path would show first.

global _start

%include "header.inc"

    sub esp, 80

    mov dword [esp], 0x00000000           ; 10.0
    mov dword [esp+4], 0x40240000
    mov dword [esp+8], 0x00000800         ; 3 + 2^-40
    mov dword [esp+12], 0x40080000
    mov dword [esp+16], 0x00000000        ; 4.0
    mov dword [esp+20], 0x40100000
    mov dword [esp+24], 0x00000000        ; 2^-25
    mov dword [esp+28], 0x3e600000
    mov dword [esp+32], 1                 ; the integer 1

    fstcw [esp+40]
    mov ax, [esp+40]
    and ax, ~0x300
    mov [esp+44], ax
    fldcw [esp+44]

    ; fprem(10.0, 3 + 2^-40): quotient trunc(10 / (3 + 2^-40)) = 3, remainder
    ; 10 - 3 * (3 + 2^-40) = 1 - 3*2^-40, exact in a double. A 24-bit-narrowed
    ; product would round 3 * (3 + 2^-40) to 9.0 and leave the remainder 1.0.
    fld qword [esp+8]                     ; st1 = 3 + 2^-40
    fld qword [esp]                       ; st0 = 10.0
    fprem
    fstp qword [esp+48]

    ; fyl2x(y = 3 + 2^-40, x = 4.0) = y * log2(4) = 2y, exact in a double. A
    ; 24-bit-narrowed multiply would round the 2^-40 term away.
    fld qword [esp+8]                     ; st1 = y = 3 + 2^-40
    fld qword [esp+16]                    ; st0 = x = 4.0
    fyl2x
    fstp qword [esp+56]

    ; fiadd dword: 2^-25 + 1 rounds to 1.0 at 24 bits (the ULP at 1.0 is
    ; 2^-23, twice 2^-25), through fpu_fadd directly -- the interpreter never
    ; takes the inline arm for this form.
    fld qword [esp+24]                    ; 2^-25
    fiadd dword [esp+32]
    fstp qword [esp+64]

    fldcw [esp+40]

%include "footer.inc"
