MEMORY
{
  /* nRF52840 with the Adafruit nRF52 UF2 bootloader.
     The app starts at 0x1000. Flash above 0xA0000 is reserved: RMK's storage
     (keymaps / BLE bonds) at 0xA0000..0xA8000 and the bootloader at 0xF4000. */
  FLASH : ORIGIN = 0x00001000, LENGTH = 636K
  RAM   : ORIGIN = 0x20000008, LENGTH = 255K
}
