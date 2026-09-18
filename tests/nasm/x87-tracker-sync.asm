; The stack top and the empty mask crossing a helper in the middle of a
; block: a helper that reads them where generated code left them, a helper
; that moves them and an inline form that must see the move. Every value
; here is exact in a double except the 80-bit one, which no double holds and
; so keeps every form that reads it on its helper arm. Each block opens with
; `finit` (TOP 0, every tag empty, status and control word reset) so its
; final register state depends only on that block's own instructions, not on
; where an earlier block happened to leave the physical register file --
; carrying state across blocks is exactly the bookkeeping this fixture once
; got wrong by hand. The `emms` block leaves a register pushed at its end, so
; the block after it opens with a `finit` that has to reset a non-zero top
; instead of a no-op -- the one case the other blocks, all draining back to
; top 0 before their own next `finit`, cannot tell apart from a missing
; reload of the wrap's spilled locals.

global _start

%include "header.inc"

    sub esp, 256

    mov dword [esp], 0x3fc00000           ; 1.5
    mov dword [esp+4], 0x40100000         ; 2.25
    mov dword [esp+8], 0x40f00000         ; 7.5

    ; 1.5 with the lowest mantissa bit set: no double holds it
    mov dword [esp+16], 0x00000001
    mov dword [esp+20], 0xc0000000
    mov word [esp+24], 0x3fff

    mov word [esp+28], 0x037f             ; the control word as reset leaves it

    ; a non-zero top across helpers: a control-word load, fxam reading st(0)
    ; and fnstsw reading the top, between inline forms
    finit
    fld dword [esp]                       ; 1.5, top 7
    fld dword [esp+4]                     ; 2.25, top 6
    fldcw [esp+28]
    fxam                                  ; a normal, positive: C2
    xor eax, eax
    fnstsw ax
    mov [esp+128], eax
    fmul dword [esp+8]                    ; 16.875
    fstp dword [esp+132]
    fstp dword [esp+136]                  ; 1.5, top 0

    ; a helper that pops: fstp st0 on the 80-bit value, then an inline push
    ; into the slot it freed
    finit
    fld dword [esp]                       ; 1.5, top 7
    fld tword [esp+16]                    ; top 6
    fstp st0                              ; top 7
    fld dword [esp+4]                     ; 2.25 at top 6
    fstp dword [esp+140]
    fstp dword [esp+144]                  ; 1.5, top 0

    ; a helper that pops twice: fucompp against the 80-bit value
    finit
    fld tword [esp+16]                    ; top 7
    fld dword [esp+4]                     ; 2.25, top 6
    fucompp                               ; 2.25 above 1.5: no condition bit, top 0
    xor eax, eax
    fnstsw ax
    mov [esp+148], eax

    ; an exchange inline, then one by helper, each followed by an inline store
    finit
    fld dword [esp]                       ; 1.5, top 7
    fld dword [esp+4]                     ; 2.25, top 6
    fxch st1                              ; st0 1.5, st1 2.25
    fstp dword [esp+152]                  ; 1.5, top 7
    fld tword [esp+16]                    ; top 6
    fxch st1                              ; st0 2.25, st1 the 80-bit value
    fstp dword [esp+156]                  ; 2.25, top 7
    fstp st0                              ; top 0

    ; the top moved by fdecstp and fincstp, and an inline push between them.
    ; The status word's TOP field is checked right after each pointer move
    ; (masked to bits 13:11, nothing else touched by fdecstp/fincstp) so a
    ; mismatch names the instruction, not the fixture's end state.
    finit
    fld dword [esp]                       ; 1.5, top 7
    fld dword [esp+4]                     ; 2.25, top 6
    fdecstp                               ; top 5, its slot empty
    xor eax, eax
    fnstsw ax
    and eax, 0x3800
    mov [esp+160], eax                    ; TOP 5 -> 0x2800
    fld dword [esp+8]                     ; 7.5 at top 4
    fstp dword [esp+164]                  ; top 5
    fincstp                               ; top 6: 2.25 is st(0) again
    xor eax, eax
    fnstsw ax
    and eax, 0x3800
    mov [esp+168], eax                    ; TOP 6 -> 0x3000
    fstp dword [esp+172]                  ; 2.25, top 7
    fstp dword [esp+176]                  ; 1.5, top 0

    ; emms after a push: AMD APM Vol. 1 §5.12 clears TOP on every 64-bit
    ; media instruction including EMMS itself; a missing reset in instr_0F77
    ; shows here as TOP 7 instead of 0. The block ends with a second push
    ; left on the stack -- top nonzero -- so the next block's finit is not a
    ; no-op.
    finit
    fld dword [esp]                       ; 1.5, top 7
    emms
    xor eax, eax
    fnstsw ax
    and eax, 0x3800
    mov [esp+180], eax                    ; TOP after emms -> 0x0
    fld dword [esp+4]                     ; 2.25, top 7: left pushed for the
                                           ; next block's finit to reset

    ; the register left pushed above means this finit has real work -- it
    ; must reset a non-zero top, not repeat what was already 0. Then the
    ; fixture compares the x87 registers and TOP rather than the MMX aliases
    ; (run.js compares mm* when the tag word is all-empty)
    finit
    fld dword [esp]                       ; 1.5, top 7: the final, compared state

%include "footer.inc"
