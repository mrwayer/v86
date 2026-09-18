; A pop or an ffree leaves the tag word of the register it emptied standing
; -- real hardware leaves a popped register's bits standing too, which
; fsave-style introspection still reads, so the tag word is never the whole
; answer generated code trusts. gen_fpu_tag_ok reads the register's index
; and its tag word both, cheaply, from an index the caller already holds
; rather than one recovered from the address; this pins that the decision
; the two together reach is still the right one for every writer that marks
; a slot empty. Each block below tags a register, empties it through one
; writer without reloading it, then compares a still-valid register against
; the emptied slot by index -- which must read as unordered (the
; stack-fault bits set) rather than as an ordinary numeric compare on the
; stale bits standing there. The last block reloads the slot a pop just
; freed and reads it inline, pinning that the ordinary reload-and-use path
; is unaffected.
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
    ; into, so this must read back correctly regardless of the stale bits
    ; the pop left standing underneath it.
    fld dword [esp+4]                     ; reload 2.25 into the slot just freed
    fchs                                  ; -2.25, read inline
    fstp dword [esp+24]                   ; -2.25

%include "footer.inc"
