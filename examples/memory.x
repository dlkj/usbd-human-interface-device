MEMORY {
    BOOT2  : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH  : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100
    PANDUMP: ORIGIN = 0x20000000, LENGTH = 1K
    RAM    : ORIGIN = 0x20000400, LENGTH = 263K
    
}

_panic_dump_start = ORIGIN(PANDUMP);
_panic_dump_end   = ORIGIN(PANDUMP) + LENGTH(PANDUMP);

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    /* ### Boot loader */
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;