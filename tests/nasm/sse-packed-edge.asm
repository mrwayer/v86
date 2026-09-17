; The packed single-precision forms on the operands that decide their
; semantics: minps and maxps with a NaN on either side and with two zeros of
; opposite sign (the source, every time), cmpps under each of its eight
; predicates against a NaN lane, subps and divps in their operand order, and
; shufps between two different registers. Add and mul, packed and scalar,
; with a distinct payload on each side: SSE keeps the destination's NaN when
; both operands are NaN (SDM Table 4-7), so the surviving payload tells the
; two operand orders apart, which two equal payloads cannot. The registers
; are compared exactly at the end.

global _start

%include "header.inc"

    sub esp, 192

    mov dword [esp], 0x3f800000           ; 1.0
    mov dword [esp+4], 0x7fc00000         ; a quiet NaN
    mov dword [esp+8], 0x00000000         ; +0.0
    mov dword [esp+12], 0x40a00000        ; 5.0
    mov dword [esp+16], 0x7fc00000        ; a quiet NaN
    mov dword [esp+20], 0x3f800000        ; 1.0
    mov dword [esp+24], 0x80000000        ; -0.0
    mov dword [esp+28], 0x40000000        ; 2.0
    movups xmm0, [esp]                    ; 1.0, NaN, +0.0, 5.0
    movups xmm1, [esp+16]                 ; NaN, 1.0, -0.0, 2.0

    movaps xmm2, xmm0
    minps xmm2, xmm1                      ; NaN, 1.0, -0.0, 2.0
    movaps xmm3, xmm0
    maxps xmm3, xmm1                      ; NaN, 1.0, -0.0, 5.0
    movaps xmm4, xmm1
    minps xmm4, xmm0                      ; 1.0, NaN, +0.0, 2.0
    movaps xmm5, xmm1
    maxps xmm5, xmm0                      ; 1.0, NaN, +0.0, 5.0

    movaps xmm6, xmm0
    subps xmm6, xmm1                      ; NaN, NaN, +0.0, 3.0
    movaps xmm7, xmm0
    divps xmm7, xmm1                      ; NaN, NaN, -0.0, 2.5

    movaps xmm2, xmm0
    shufps xmm2, xmm1, 0xb4               ; 1.0, NaN, 2.0, -0.0
    movups [esp+32], xmm2
    movaps xmm2, xmm1
    shufps xmm2, xmm0, 0x1b               ; 2.0, -0.0, NaN, 1.0
    movups [esp+48], xmm2

    ; the eight predicates, each result's low lane kept in memory
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 0                   ; eq: 0, 0, -1, 0
    movd [esp], xmm2
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 1                   ; lt: 0, 0, 0, 0
    movd [esp+4], xmm2
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 2                   ; le: 0, 0, -1, 0
    movd [esp+8], xmm2
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 3                   ; unord: -1, -1, 0, 0
    movd [esp+12], xmm2
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 4                   ; neq: -1, -1, 0, -1
    movd [esp+16], xmm2
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 5                   ; nlt: -1, -1, -1, -1
    movd [esp+20], xmm2
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 6                   ; nle: -1, -1, 0, -1
    movd [esp+24], xmm2
    movaps xmm2, xmm0
    cmpps xmm2, xmm1, 7                   ; ord: 0, 0, -1, -1
    movd [esp+28], xmm2

    ; add and mul, packed and scalar, on two NaNs with distinct payloads: the
    ; destination's payload (...001) must be the one that survives, never the
    ; source's (...002).
    mov dword [esp+64], 0x7fc00001        ; regA: a quiet NaN, payload 1
    mov dword [esp+68], 0x40000000        ; 2.0
    mov dword [esp+72], 0x40000000        ; 2.0
    mov dword [esp+76], 0x40000000        ; 2.0
    mov dword [esp+80], 0x7fc00002        ; regB: a quiet NaN, payload 2
    mov dword [esp+84], 0x40400000        ; 3.0
    mov dword [esp+88], 0x40400000        ; 3.0
    mov dword [esp+92], 0x40400000        ; 3.0

    movups xmm2, [esp+64]
    addps xmm2, [esp+80]                  ; payload 1, 5.0, 5.0, 5.0
    movups [esp+96], xmm2
    movups xmm2, [esp+64]
    mulps xmm2, [esp+80]                  ; payload 1, 6.0, 6.0, 6.0
    movups [esp+112], xmm2
    movups xmm2, [esp+64]
    addss xmm2, [esp+80]                  ; payload 1, 2.0, 2.0, 2.0 (upper lanes regA's)
    movups [esp+128], xmm2
    movups xmm2, [esp+64]
    mulss xmm2, [esp+80]                  ; payload 1, 2.0, 2.0, 2.0
    movups [esp+144], xmm2

    mov dword [esp+160], 0x00000001       ; a quiet double NaN, payload 1
    mov dword [esp+164], 0x7ff80000
    mov dword [esp+168], 0x00000002       ; a quiet double NaN, payload 2
    mov dword [esp+172], 0x7ff80000

    movsd xmm2, [esp+160]
    addsd xmm2, [esp+168]                 ; payload 1
    movsd [esp+176], xmm2
    movsd xmm2, [esp+160]
    mulsd xmm2, [esp+168]                 ; payload 1
    movsd [esp+184], xmm2

%include "footer.inc"
