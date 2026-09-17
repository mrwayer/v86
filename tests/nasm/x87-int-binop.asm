; fiadd, fimul, fisub, fisubr, fidiv and fidivr against a word and a half in
; memory, results stored as doubles, and the two results the inline arm must
; leave to the library: a product past the double's range, which the 80-bit
; format still holds and is stored as such, and a quotient below it.

global _start

%include "header.inc"

    sub esp, 160

    mov dword [esp], 7                    ; a word
    mov word [esp+4], -3                  ; a half
    mov dword [esp+8], 0x00000000         ; 1.5
    mov dword [esp+12], 0x3ff80000
    mov dword [esp+16], 0x00000000        ; 2^1023
    mov dword [esp+20], 0x7fe00000
    mov dword [esp+32], 2

    fld qword [esp+8]                     ; 1.5
    fiadd dword [esp]                     ; 8.5
    fstp qword [esp+40]
    fld qword [esp+8]
    fimul dword [esp]                     ; 10.5
    fstp qword [esp+48]
    fld qword [esp+8]
    fisub dword [esp]                     ; -5.5
    fstp qword [esp+56]
    fld qword [esp+8]
    fisubr dword [esp]                    ; 5.5
    fstp qword [esp+64]
    fld qword [esp+8]
    fidiv dword [esp+32]                  ; 0.75
    fstp qword [esp+72]
    fld qword [esp+8]
    fidivr dword [esp+32]                 ; 1.3333...
    fstp qword [esp+80]

    fld qword [esp+8]
    fiadd word [esp+4]                    ; -1.5
    fstp qword [esp+88]
    fld qword [esp+8]
    fimul word [esp+4]                    ; -4.5
    fstp qword [esp+96]
    fld qword [esp+8]
    fisub word [esp+4]                    ; 4.5
    fstp qword [esp+104]
    fld qword [esp+8]
    fisubr word [esp+4]                   ; -4.5
    fstp qword [esp+112]
    fld qword [esp+8]
    fidiv word [esp+4]                    ; -0.5
    fstp qword [esp+120]
    fld qword [esp+8]
    fidivr word [esp+4]                   ; -2.0
    fstp qword [esp+128]

    ; past the double's range and below it
    fld qword [esp+16]                    ; 2^1023
    fimul dword [esp+32]                  ; 2^1024
    fstp tword [esp+136]
    fld qword [esp+16]
    fidivr word [esp+4]                   ; -3 * 2^-1023, a double's denormal
    fstp qword [esp+152]

%include "footer.inc"
