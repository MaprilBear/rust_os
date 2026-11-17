/* Sized to match the nRF52840 (1 MiB flash at 0x0, 256 KiB RAM at 0x2000_0000).
 * QEMU's mps2-an386 maps 4 MiB at both base addresses, so this layout also runs
 * there; the sizes are just a smaller window of what QEMU provides.
 *
 * Kernel/user split for the MPU (see mpu_init):
 *   KFLASH / KRAM  - privileged-only regions (reachable only from handler mode,
 *                    once thread mode is dropped to unprivileged).
 *   UFLASH / URAM  - accessible to unprivileged thread code.
 *
 * MPU constraints: each region's size is a power of two and its base is aligned
 * to that size. KFLASH is 64 KiB at 0x0 and KRAM is 32 KiB at 0x2000_0000, which
 * match MPU regions 1 and 3. Keep them in sync with mpu_init.
 *
 * Everything lands in the kernel regions by default. User code and data opt in
 * with #[link_section = ".user_text"], ".user_rodata", ".user_data" or
 * ".user_bss".
 */
MEMORY
{
    KFLASH (rx)  : ORIGIN = 0x00000000, LENGTH = 64K
    UFLASH (rx)  : ORIGIN = 0x00010000, LENGTH = 960K
    KRAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 32K
    URAM   (rwx) : ORIGIN = 0x20008000, LENGTH = 224K
}

ENTRY(ResetHandler)

SECTIONS
{
    /* ---------------- Kernel (privileged-only) ---------------- */

    .vector_table ORIGIN(KFLASH) :
    {
        KEEP(*(.vector_table .vector_table.*))
    } > KFLASH

    .text :
    {
        *(.text .text.*)
    } > KFLASH

    .rodata :
    {
        *(.rodata .rodata.*)
        . = ALIGN(4);
    } > KFLASH

    .data : ALIGN(4)
    {
        _sdata = .;
        /* Kernel semaphore objects (see semaphore!). They sit first so the
         * kernel can validate a user-supplied handle as "inside [_sksem, _eksem)
         * and a multiple of the object size". Initialised by the normal .data copy. */
        _sksem = .;
        *(.ksem .ksem.*)
        _eksem = .;
        *(.data .data.*)
        . = ALIGN(4);
        _edata = .;
    } > KRAM AT > KFLASH
    _sidata = LOADADDR(.data);

    .bss (NOLOAD) : ALIGN(4)
    {
        _sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        . = ALIGN(4);
        _ebss = .;
    } > KRAM

    /* MSP (handler-mode stack) grows down from the top of KRAM. */
    _estack = ORIGIN(KRAM) + LENGTH(KRAM);

    /* ---------------- User (unprivileged) ---------------- */

    .user_text : ALIGN(4)
    {
        _suser_text = .;
        *(.user_text .user_text.*)
        . = ALIGN(4);
        _euser_text = .;
    } > UFLASH

    .user_rodata : ALIGN(4)
    {
        *(.user_rodata .user_rodata.*)
        . = ALIGN(4);
    } > UFLASH

    .user_data : ALIGN(4)
    {
        _sudata = .;
        *(.user_data .user_data.*)
        . = ALIGN(4);
        _eudata = .;
    } > URAM AT > UFLASH
    _siudata = LOADADDR(.user_data);

    .user_bss (NOLOAD) : ALIGN(4)
    {
        _subss = .;
        *(.user_bss .user_bss.*)
        . = ALIGN(4);
        _eubss = .;
    } > URAM

    /* The rest of URAM is the user heap. Thread stacks (PSP) are carved out of
     * it by the kernel's second allocator, so they are unprivileged-accessible. */
    _suser_heap = ALIGN(8);
    _euser_heap = ORIGIN(URAM) + LENGTH(URAM);

    /* Exported so mpu_init can derive region bases/sizes from the linker. */
    _kflash_base = ORIGIN(KFLASH);
    _kflash_size = LENGTH(KFLASH);
    _kram_base   = ORIGIN(KRAM);
    _kram_size   = LENGTH(KRAM);

    /* Ranges the kernel uses to validate user pointers passed to syscalls. */
    _flash_base  = ORIGIN(KFLASH);
    _flash_size  = LENGTH(KFLASH) + LENGTH(UFLASH);
    _uram_base   = ORIGIN(URAM);
    _uram_size   = LENGTH(URAM);

    /DISCARD/ :
    {
        *(.ARM.exidx .ARM.exidx.*)
        *(.ARM.extab .ARM.extab.*)
    }
}

/* The MPU regions assume these exact power-of-two, size-aligned windows. */
ASSERT(ORIGIN(KFLASH) == 0x00000000 && LENGTH(KFLASH) == 64K,  "KFLASH must match MPU region 1 (64 KiB @ 0x0)")
ASSERT(ORIGIN(KRAM) == 0x20000000 && LENGTH(KRAM) == 32K,      "KRAM must match MPU region 3 (32 KiB @ 0x2000_0000)")
ASSERT(_estack <= ORIGIN(KRAM) + LENGTH(KRAM),                 "MSP stack must stay inside KRAM")
