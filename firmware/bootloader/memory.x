/* Sector 0 of the STM32F769 (32 KB). Everything else is laid out in ../stm32f769i-disco/README.md. */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 32K
  RAM   : ORIGIN = 0x20000000, LENGTH = 64K
}
