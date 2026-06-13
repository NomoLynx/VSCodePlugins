.data
    a0: .word 0
    b0: .word 1

.text
main:
    la t0, b0
    lw t1, 0(t0)
