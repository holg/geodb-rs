/* Linked *before* link.x (so its patterns win over .bss). The Ethernet DMA does not see the data
   cache: its descriptors and the stack storage sit in SRAM1 at a fixed address that display.rs
   maps non-cacheable (an MPU region). Only those go there: LDREX/STREX (atomics) fail on
   non-cacheable memory on the F7, so .bss and the rest stay where they were.

   The stack's packet pool (xarxa-driver's POOL, the frames the DMA reads and writes) must not come
   here: its allocation bitmap is atomic (CAS), and this section is NOLOAD, never zeroed, so the
   bitmap would start with whatever SRAM1 held (after a reset: the last run's buffers, all in use).
   It stays in .bss, in the DTCM: zeroed at start, not cached, atomics work, and the Ethernet DMA
   reaches it through the Cortex-M7's AHBS port. */
MEMORY
{
  ETH (rw) : ORIGIN = 0x20060000, LENGTH = 64K
}

SECTIONS
{
  .ethbuf (NOLOAD) : ALIGN(32)
  {
    *(.ethbuf .ethbuf.*)
  } > ETH
}
