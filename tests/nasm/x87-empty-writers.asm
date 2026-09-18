; The invariant this branch adds: a writer that marks an x87 register empty
; (fpu_pop, fpu_ffree, fninit, ...) also clears that register's tag word, so
; gen_fpu_tag_ok and the interpreter's fpu_tagged can trust the tag word
; alone rather than also reading the empty bit a pop would otherwise leave
; stale. Each block below tags a register, empties it through one writer
; without reloading it, then compares a still-valid register against it by
; index -- which must read as unordered (the stack-fault bits set) rather
; than as an ordinary numeric compare, since the writer's own tag word is
; what generated code trusts. The last block reloads the slot a pop just
; freed and reads it inline, pinning that the ordinary reload-and-use path
; still works once the empty check is gone from it.
;
; Every stored status word drops C1, PE, OE and DE, the bits the harness
; does not compare either, so the fixture pins the bits the emulator
; implements and no other.

global _start

%include "header.inc"

    sub esp, 32

    mov dword [esp], 0x3fc00000           ; 1.5
    mov dword [esp+4], 0x40100000         ; 2.25

    finit

    ; fpu_pop: tag two registers, pop the top one (freeing and re-tagging
    ; nothing else), then compare the surviving register against the slot
    ; just popped, addressed by its now-relative index.
    fld dword [esp]                       ; st0 = 1.5
    fld dword [esp+4]                     ; st0 = 2.25, st1 = 1.5
    fstp st0                              ; store st0 into itself, then pop: that slot is now empty
    xor eax, eax
    fcom st7                              ; st0 (1.5, valid) against the slot fpu_pop just emptied
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+8], eax
    fstp dword [esp+12]                   ; drains st0 (1.5), popping again

    ; fpu_ffree: tag two registers, ffree the deeper one by index (no pop,
    ; the pointer does not move), then compare the top against it by the
    ; same index it was freed at.
    fld dword [esp]                       ; st0 = 1.5
    fld dword [esp+4]                     ; st0 = 2.25, st1 = 1.5
    ffree st1                             ; frees st1's physical slot; the pointer is unchanged
    xor eax, eax
    fcom st1                              ; st0 (2.25, valid) against the slot ffree just emptied
    fnstsw ax
    and eax, 0xFDD5
    mov [esp+16], eax
    fstp dword [esp+20]                   ; drains st0 (2.25), popping the slot fpu_ffree left standing

    ; the ordinary path: the slot the drain above just popped, reloaded and
    ; read inline -- fld always writes a fresh tag on the slot it pushes
    ; into, so this must read back correctly whichever way the invariant
    ; above is implemented.
    fld dword [esp+4]                     ; reload 2.25 into the slot just freed
    fchs                                  ; -2.25, read inline
    fstp dword [esp+24]                   ; -2.25

%include "footer.inc"
