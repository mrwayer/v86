global _start

%include "header.inc"

    cld

    ; forward: run the probe once so its old bytes are seen before the patch,
    ; overwrite it with rep stosb going forward, then run it again. A dirty
    ; range that misses a written byte leaves the stale "mov eax, imm32"
    ; compiled in, instead of the nops it was patched with.
    call fwd_probe

    mov edi, fwd_probe
    mov ecx, fwd_probe_end - fwd_probe
    mov al, 90h
    rep stosb

    mov eax, 0deadbeefh
    call fwd_probe
    mov edx, eax            ; 0deadbeefh if the patch was seen, 11111111h if stale

    ; backward: same probe shape, patched with std + rep stosb writing from the
    ; high end of the probe down to its first byte.
    call bwd_probe

    mov edi, bwd_probe_end - 1
    mov ecx, bwd_probe_end - bwd_probe
    mov al, 90h
    std
    rep stosb
    cld

    mov eax, 0deadbeefh
    call bwd_probe
    mov ebx, eax             ; 0deadbeefh if the patch was seen, 22222222h if stale

    jmp done

fwd_probe:
    mov eax, 11111111h
    ret
fwd_probe_end:

bwd_probe:
    mov eax, 22222222h
    ret
bwd_probe_end:

done:

%include "footer.inc"
