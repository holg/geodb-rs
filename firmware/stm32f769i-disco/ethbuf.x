/* The Ethernet DMA cannot reach the DTCM (0x20000000..0x20020000) and does not see the data
   cache: its packet buffers sit in SRAM1 (64 KB) at a fixed address that main.rs maps non-cacheable. */
SECTIONS
{
  .ethbuf 0x20060000 (NOLOAD) : ALIGN(32)
  {
    *(.ethbuf .ethbuf.*)
    /* the stack's global packet pool (xarxa-driver) */
    *(.bss._ZN12xarxa_driver3buf4POOL*)
  } > RAM
}
INSERT BEFORE .bss;
