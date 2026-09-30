/* Linked *before* link.x (so its patterns win over .bss). The Ethernet DMA cannot reach the DTCM (0x20000000..0x20020000) and does not see the data
   cache: its packet buffers sit in SRAM1 at a fixed address that display.rs maps non-cacheable
   (an MPU region). Only those buffers go there: LDREX/STREX (the executor's atomics) fault on
   anything but TCM or cacheable memory on the F7, so .bss and the rest stay where they were.
   NOLOAD and outside .bss: cortex-m-rt does not zero it, main.rs does (the pool's bitmap must start
   at zero, and SRAM1 keeps the last run's bits across a reset). */
MEMORY
{
  ETH (rw) : ORIGIN = 0x20060000, LENGTH = 64K
}

SECTIONS
{
  .ethbuf (NOLOAD) : ALIGN(32)
  {
    __sethbuf = .;
    *(.ethbuf .ethbuf.*)
    /* the stack's global packet pool (xarxa-driver) */
    *(.bss._ZN12xarxa_driver3buf4POOL*)
    . = ALIGN(4);
    __eethbuf = .;
  } > ETH
}
