# The bundled libsodium static library for x86_64-pc-windows-gnu expects the
# C23 `memset_explicit` symbol, which older MinGW runtimes do not provide.
# This assembly object supplies the standard semantics (write bytes and do not
# let an optimizer elide the stores). It is only linked on Windows GNU.
    .text
    .globl memset_explicit
    .def memset_explicit; .scl 2; .type 32; .endef
memset_explicit:
    movq %rcx, %rax
    testq %r8, %r8
    je .Lmemset_done
.Lmemset_loop:
    movb %dl, (%rcx)
    incq %rcx
    decq %r8
    jne .Lmemset_loop
.Lmemset_done:
    ret
