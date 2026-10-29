/* Sized to match the nRF52840 (1 MiB flash at 0x0, 256 KiB RAM at 0x2000_0000).
 * QEMU's mps2-an386 maps 4 MiB at both base addresses, so this layout also runs
 * there; the sizes are just a smaller window of what QEMU provides.
 */
MEMORY
{
    FLASH (rx)  : ORIGIN = 0x00000000, LENGTH = 1M
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 256K
}

ENTRY(Reset)

SECTIONS
{
    .vector_table ORIGIN(FLASH) :
    {
        KEEP(*(.vector_table .vector_table.*))
    } > FLASH

    .text :
    {
        *(.text .text.*)
    } > FLASH

    .rodata :
    {
        *(.rodata .rodata.*)
        . = ALIGN(4);
    } > FLASH

    .data : ALIGN(4)
    {
        _sdata = .;
        *(.data .data.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT > FLASH
    _sidata = LOADADDR(.data);

    .bss (NOLOAD) : ALIGN(4)
    {
        _sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4);
        _ebss = .;
    } > RAM

    _estack = ORIGIN(RAM) + LENGTH(RAM);

    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.*)
        *(.ARM.extab .ARM.extab.*)
    }
}
