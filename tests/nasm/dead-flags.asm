global _start

%include "header.inc"

    ; A writer whose flags are immediately overwritten before anything reads
    ; them: JIT_DEAD_FLAGS should let the writer skip its flag records, and
    ; the reader must still see the overwriter's flags, not stale ones. Each
    ; writer/overwriter pair below is chosen so their flags actually differ
    ; (most often ZF), so a leaked stale flag is visible in the result.

    ; writer, overwriter, reader: pushf
    mov eax, 5
    add eax, 0
    add eax, -5
    pushf
    and dword [esp], 8ffh

    ; writer, overwriter, reader: cmp + jcc
    mov eax, 3
    sub eax, 3
    cmp eax, 1
    jae .cmp_jcc_ge
    mov ebx, 1
    jmp .cmp_jcc_done
.cmp_jcc_ge:
    mov ebx, 2
.cmp_jcc_done:

    ; writer, overwriter, reader: setcc
    mov ecx, 5
    mov edx, 0
    inc ecx
    test edx, edx
    setnz dl

    ; writer, overwriter, reader: lahf
    mov eax, 0
    and eax, 0
    or eax, 1
    lahf

    ; the eight-instruction walk limit: a writer, seven Leaves, an
    ; overwriter right at the walk's edge, then a reader
    mov eax, 5
    add eax, 0
    mov ebx, ecx
    mov ebx, ecx
    mov ebx, ecx
    mov ebx, ecx
    mov ebx, ecx
    mov ebx, ecx
    mov ebx, ecx
    sub eax, 5
    pushf
    and dword [esp], 8ffh

    ; a block boundary between the writer and its overwriter: the walk must
    ; not cross it, so the writer's flags stay live here even though nothing
    ; before the jump reads them either
    mov eax, 2
    add eax, 0
    jmp .block_boundary_overwrite
.block_boundary_overwrite:
    sub eax, 2
    pushf
    and dword [esp], 8ffh

%include "footer.inc"
