#![allow(dead_code)]
#![allow(non_upper_case_globals)]

use core::ptr;

pub const GPIO_OUT_PIN0: u32 = 1 << 0;
pub const GPIO_OUT_PIN1: u32 = 1 << 1;
pub const GPIO_OUT_PIN2: u32 = 1 << 2;
pub const GPIO_OUT_PIN3: u32 = 1 << 3;
pub const GPIO_OUT_PIN4: u32 = 1 << 4;
pub const GPIO_OUT_PIN5: u32 = 1 << 5;
pub const GPIO_OUT_PIN6: u32 = 1 << 6;
pub const GPIO_OUT_PIN7: u32 = 1 << 7;
pub const GPIO_OUT_PIN8: u32 = 1 << 8;
pub const GPIO_OUT_PIN9: u32 = 1 << 9;
pub const GPIO_OUT_PIN10: u32 = 1 << 10;
pub const GPIO_OUT_PIN11: u32 = 1 << 11;
pub const GPIO_OUT_PIN12: u32 = 1 << 12;
pub const GPIO_OUT_PIN13: u32 = 1 << 13;
pub const GPIO_OUT_PIN14: u32 = 1 << 14;
pub const GPIO_OUT_PIN15: u32 = 1 << 15;
pub const GPIO_OUT_PIN16: u32 = 1 << 16;
pub const GPIO_OUT_PIN17: u32 = 1 << 17;
pub const GPIO_OUT_PIN18: u32 = 1 << 18;
pub const GPIO_OUT_PIN19: u32 = 1 << 19;
pub const GPIO_OUT_PIN20: u32 = 1 << 20;
pub const GPIO_OUT_PIN21: u32 = 1 << 21;
pub const GPIO_OUT_PIN22: u32 = 1 << 22;
pub const GPIO_OUT_PIN23: u32 = 1 << 23;
pub const GPIO_OUT_PIN24: u32 = 1 << 24;
pub const GPIO_OUT_PIN25: u32 = 1 << 25;
pub const GPIO_OUT_PIN26: u32 = 1 << 26;
pub const GPIO_OUT_PIN27: u32 = 1 << 27;
pub const GPIO_OUT_PIN28: u32 = 1 << 28;
pub const GPIO_OUT_PIN29: u32 = 1 << 29;
pub const GPIO_OUT_PIN30: u32 = 1 << 30;
pub const GPIO_OUT_PIN31: u32 = 1 << 31;

pub const GPIO_PIN_CNF_DRIVE_S0S1: u32 = 0;
pub const GPIO_PIN_CNF_DRIVE_H0H1: u32 = 1;
pub const GPIO_PIN_CNF_DRIVE_S0H1: u32 = 2;
pub const GPIO_PIN_CNF_DRIVE_H0S1: u32 = 3;
pub const GPIO_PIN_CNF_DRIVE_D0S1: u32 = 4;
pub const GPIO_PIN_CNF_DRIVE_S0D1: u32 = 5;
pub const GPIO_PIN_CNF_DRIVE_H0D1: u32 = 6;
pub const GPIO_PIN_CNF_DRIVE_D0H1: u32 = 7;

pub const GPIO_PIN_CNF_INPUT_CONNECT_Disconnected: u32 = 0;
pub const GPIO_PIN_CNF_INPUT_CONNECT_Connected: u32 = 1;

pub const GPIO_PIN_CNF_PULL_Disabled: u32 = 0;
pub const GPIO_PIN_CNF_PULL_Pulldown: u32 = 1;
pub const GPIO_PIN_CNF_PULL_Pullup: u32 = 3;

pub const GPIO_PIN_CNF_DIR_Input: u32 = 0;
pub const GPIO_PIN_CNF_DIR_Output: u32 = 1;

// UARTE
pub const UARTE_ENABLE_ENABLE_Disabled: u32 = 0;
pub const UARTE_ENABLE_ENABLE_Enabled: u32 = 1;

pub const UARTE_CONFIG_PARITY_Excluded: u32 = 0;
pub const UARTE_CONFIG_PARITY_Included: u32 = 1;

pub const UARTE_CONFIG_HWFC_Disabled: u32 = 0;
pub const UARTE_CONFIG_HWFC_Enabled: u32 = 1;

pub const UARTE_CONFIG_PARITY_Excluded_0: u32 = 0;
pub const UARTE_CONFIG_PARITY_Excluded_1: u32 = 0;
pub const UARTE_CONFIG_HWFC_Disabled_0: u32 = 0;
pub const UARTE_CONFIG_HWFC_Disabled_1: u32 = 0;

// GPIOTE
pub const GPIOTE_CONFIG_POLARITY_None: u32 = 0;
pub const GPIOTE_CONFIG_POLARITY_LoToHi: u32 = 1;
pub const GPIOTE_CONFIG_POLARITY_HiToLo: u32 = 2;
pub const GPIOTE_CONFIG_POLARITY_Toggle: u32 = 3;

pub const GPIOTE_CONFIG_MODE_Disabled: u32 = 0;
pub const GPIOTE_CONFIG_MODE_Event: u32 = 1;
pub const GPIOTE_CONFIG_MODE_Task: u32 = 3;

pub const GPIOTE_CONFIG_OUTINIT_Low: u32 = 0;
pub const GPIOTE_CONFIG_OUTINIT_High: u32 = 1;

// TIMER / RTC
pub const TIMER_TASKS_START_Trigger: u32 = 1;
pub const TIMER_TASKS_STOP_Trigger: u32 = 1;
pub const TIMER_TASKS_CLEAR_Trigger: u32 = 1;

pub const RTC_TASKS_START_Trigger: u32 = 1;
pub const RTC_TASKS_STOP_Trigger: u32 = 1;
pub const RTC_TASKS_CLEAR_Trigger: u32 = 1;

pub const TIMER_INTENSET_COMPARE0_Set: u32 = 1 << 16;
pub const TIMER_INTENSET_COMPARE1_Set: u32 = 1 << 17;
pub const TIMER_INTENSET_COMPARE2_Set: u32 = 1 << 18;
pub const TIMER_INTENSET_COMPARE3_Set: u32 = 1 << 19;

// POWER / CLOCK / NVMC
pub const POWER_RESETREAS_RESETPIN: u32 = 1 << 0;
pub const POWER_RESETREAS_SREQ: u32 = 1 << 1;
pub const POWER_RESETREAS_LPCOMP: u32 = 1 << 2;
pub const POWER_RESETREAS_DIF: u32 = 1 << 3;
pub const POWER_RESETREAS_NFC: u32 = 1 << 4;
pub const POWER_RESETREAS_CTRLAP: u32 = 1 << 5;
pub const POWER_RESETREAS_Unlock: u32 = 1 << 16;

pub const CLOCK_LFCLKSTAT_SRC_RC: u32 = 0;
pub const CLOCK_LFCLKSTAT_SRC_Xtal: u32 = 1;
pub const CLOCK_LFCLKSTAT_SRC_Synth: u32 = 2;
pub const CLOCK_LFCLKSTAT_SRC_External: u32 = 3;

pub const NVMC_CONFIG_WEN_Ren: u32 = 0;
pub const NVMC_CONFIG_WEN_Wen: u32 = 1;
pub const NVMC_CONFIG_WEN_Erase: u32 = 2;

pub const AAR_ENABLE_ENABLE_Disabled: u32 = 0;
pub const AAR_ENABLE_ENABLE_Enabled: u32 = 3;

pub const CCM_INTENSET_ENDKSGEN_Set: u32 = 1 << 0;
pub const CCM_INTENSET_ENDCRYPT_Set: u32 = 1 << 1;
pub const CCM_INTENSET_ERROR_Set: u32 = 1 << 2;

pub const CLOCK_PRESENT: u32 = 1;
pub const CLOCK_COUNT: u32 = 1;

pub const POWER_PRESENT: u32 = 1;
pub const POWER_COUNT: u32 = 1;
pub const POWER_FEATURE_RAM_REGISTERS_PRESENT: u32 = 1;
pub const POWER_FEATURE_RAM_REGISTERS_COUNT: u32 = 9;
pub const POWER_FEATURE_VDDH_PRESENT: u32 = 1;
pub const POWER_FEATURE_VDDH_DCDC_PRESENT: u32 = 1;

pub const NVMC_PRESENT: u32 = 1;
pub const NVMC_COUNT: u32 = 1;
pub const NVMC_FEATURE_CACHE_PRESENT: u32 = 1;

pub const GPIO_PRESENT: u32 = 1;
pub const GPIO_COUNT: u32 = 2;
pub const P0_PIN_NUM: u32 = 32;
pub const P1_PIN_NUM: u32 = 16;
pub const P0_FEATURE_PINS_PRESENT: u32 = 0xFFFF_FFFF;
pub const P1_FEATURE_PINS_PRESENT: u32 = 0x0000_FFFF;

pub const ACL_PRESENT: u32 = 1;
pub const ACL_REGIONS_COUNT: u32 = 8;

pub const RADIO_PRESENT: u32 = 1;
pub const RADIO_COUNT: u32 = 1;
pub const RADIO_EASYDMA_MAXCNT_SIZE: u32 = 8;
pub const RADIO_FEATURE_IEEE_802_15_4_PRESENT: u32 = 1;

pub const AAR_PRESENT: u32 = 1;
pub const AAR_COUNT: u32 = 1;
pub const AAR_MAX_IRK_NUM: u32 = 16;

pub const ECB_PRESENT: u32 = 1;
pub const ECB_COUNT: u32 = 1;

pub const CCM_PRESENT: u32 = 1;
pub const CCM_COUNT: u32 = 1;

pub const NFCT_PRESENT: u32 = 1;
pub const NFCT_COUNT: u32 = 1;
pub const NFCT_EASYDMA_MAXCNT_SIZE: u32 = 9;

pub const PPI_PRESENT: u32 = 1;
pub const PPI_COUNT: u32 = 1;
pub const PPI_CH_NUM: u32 = 20;
pub const PPI_FIXED_CH_NUM: u32 = 12;
pub const PPI_GROUP_NUM: u32 = 6;
pub const PPI_FEATURE_FORKS_PRESENT: u32 = 1;

pub const EGU_PRESENT: u32 = 1;
pub const EGU_COUNT: u32 = 6;
pub const EGU0_CH_NUM: u32 = 16;
pub const EGU1_CH_NUM: u32 = 16;
pub const EGU2_CH_NUM: u32 = 16;
pub const EGU3_CH_NUM: u32 = 16;
pub const EGU4_CH_NUM: u32 = 16;
pub const EGU5_CH_NUM: u32 = 16;

pub const TIMER_PRESENT: u32 = 1;
pub const TIMER_COUNT: u32 = 5;

pub const TIMER0_MAX_SIZE: u32 = 32;
pub const TIMER1_MAX_SIZE: u32 = 32;
pub const TIMER2_MAX_SIZE: u32 = 32;
pub const TIMER3_MAX_SIZE: u32 = 32;
pub const TIMER4_MAX_SIZE: u32 = 32;

pub const TIMER0_CC_NUM: u32 = 4;
pub const TIMER1_CC_NUM: u32 = 4;
pub const TIMER2_CC_NUM: u32 = 4;
pub const TIMER3_CC_NUM: u32 = 6;
pub const TIMER4_CC_NUM: u32 = 6;

pub const RTC_PRESENT: u32 = 1;
pub const RTC_COUNT: u32 = 3;
pub const RTC0_CC_NUM: u32 = 3;
pub const RTC1_CC_NUM: u32 = 4;
pub const RTC2_CC_NUM: u32 = 4;

pub const RNG_PRESENT: u32 = 1;
pub const RNG_COUNT: u32 = 1;

pub const WDT_PRESENT: u32 = 1;
pub const WDT_COUNT: u32 = 1;

pub const TEMP_PRESENT: u32 = 1;
pub const TEMP_COUNT: u32 = 1;

pub const SPI_PRESENT: u32 = 1;
pub const SPI_COUNT: u32 = 3;

pub const SPIM_PRESENT: u32 = 1;
pub const SPIM_COUNT: u32 = 4;
pub const SPIM0_MAX_DATARATE: u32 = 8;
pub const SPIM1_MAX_DATARATE: u32 = 8;
pub const SPIM2_MAX_DATARATE: u32 = 8;
pub const SPIM3_MAX_DATARATE: u32 = 32;

pub const SPIM0_FEATURE_HARDWARE_CSN_PRESENT: u32 = 0;
pub const SPIM1_FEATURE_HARDWARE_CSN_PRESENT: u32 = 0;
pub const SPIM2_FEATURE_HARDWARE_CSN_PRESENT: u32 = 0;
pub const SPIM3_FEATURE_HARDWARE_CSN_PRESENT: u32 = 1;

pub const SPIM0_FEATURE_DCX_PRESENT: u32 = 0;
pub const SPIM1_FEATURE_DCX_PRESENT: u32 = 0;
pub const SPIM2_FEATURE_DCX_PRESENT: u32 = 0;
pub const SPIM3_FEATURE_DCX_PRESENT: u32 = 1;

pub const SPIM0_FEATURE_RXDELAY_PRESENT: u32 = 0;
pub const SPIM1_FEATURE_RXDELAY_PRESENT: u32 = 0;
pub const SPIM2_FEATURE_RXDELAY_PRESENT: u32 = 0;
pub const SPIM3_FEATURE_RXDELAY_PRESENT: u32 = 1;

pub const SPIM0_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const SPIM1_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const SPIM2_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const SPIM3_EASYDMA_MAXCNT_SIZE: u32 = 16;

pub const SPIS_PRESENT: u32 = 1;
pub const SPIS_COUNT: u32 = 3;
pub const SPIS0_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const SPIS1_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const SPIS2_EASYDMA_MAXCNT_SIZE: u32 = 16;

pub const TWI_PRESENT: u32 = 1;
pub const TWI_COUNT: u32 = 2;

pub const TWIM_PRESENT: u32 = 1;
pub const TWIM_COUNT: u32 = 2;
pub const TWIM0_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const TWIM1_EASYDMA_MAXCNT_SIZE: u32 = 16;

pub const TWIS_PRESENT: u32 = 1;
pub const TWIS_COUNT: u32 = 2;
pub const TWIS0_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const TWIS1_EASYDMA_MAXCNT_SIZE: u32 = 16;

pub const UART_PRESENT: u32 = 1;
pub const UART_COUNT: u32 = 1;

pub const UARTE_PRESENT: u32 = 1;
pub const UARTE_COUNT: u32 = 2;
pub const UARTE0_EASYDMA_MAXCNT_SIZE: u32 = 16;
pub const UARTE1_EASYDMA_MAXCNT_SIZE: u32 = 16;

pub const QDEC_PRESENT: u32 = 1;
pub const QDEC_COUNT: u32 = 1;

pub const SAADC_PRESENT: u32 = 1;
pub const SAADC_COUNT: u32 = 1;
pub const SAADC_EASYDMA_MAXCNT_SIZE: u32 = 15;
pub const SAADC_CH_NUM: u32 = 8;

pub const GPIOTE_PRESENT: u32 = 1;
pub const GPIOTE_COUNT: u32 = 1;
pub const GPIOTE_CH_NUM: u32 = 8;
pub const GPIOTE_FEATURE_SET_PRESENT: u32 = 1;
pub const GPIOTE_FEATURE_CLR_PRESENT: u32 = 1;

pub const LPCOMP_PRESENT: u32 = 1;
pub const LPCOMP_COUNT: u32 = 1;
pub const LPCOMP_REFSEL_RESOLUTION: u32 = 16;
pub const LPCOMP_FEATURE_HYST_PRESENT: u32 = 1;

pub const COMP_PRESENT: u32 = 1;
pub const COMP_COUNT: u32 = 1;

pub const PWM_PRESENT: u32 = 1;
pub const PWM_COUNT: u32 = 4;
pub const PWM0_CH_NUM: u32 = 4;
pub const PWM1_CH_NUM: u32 = 4;
pub const PWM2_CH_NUM: u32 = 4;
pub const PWM3_CH_NUM: u32 = 4;
pub const PWM0_EASYDMA_MAXCNT_SIZE: u32 = 15;
pub const PWM1_EASYDMA_MAXCNT_SIZE: u32 = 15;
pub const PWM2_EASYDMA_MAXCNT_SIZE: u32 = 15;
pub const PWM3_EASYDMA_MAXCNT_SIZE: u32 = 15;

pub const PDM_PRESENT: u32 = 1;
pub const PDM_COUNT: u32 = 1;
pub const PDM_EASYDMA_MAXCNT_SIZE: u32 = 15;

pub const I2S_PRESENT: u32 = 1;
pub const I2S_COUNT: u32 = 1;
pub const I2S_EASYDMA_MAXCNT_SIZE: u32 = 14;

pub const USBD_PRESENT: u32 = 1;
pub const USBD_COUNT: u32 = 1;
pub const USBD_EASYDMA_MAXCNT_SIZE: u32 = 7;

pub const CRYPTOCELL_PRESENT: u32 = 1;
pub const CRYPTOCELL_COUNT: u32 = 1;

pub const QSPI_PRESENT: u32 = 1;
pub const QSPI_COUNT: u32 = 1;
pub const QSPI_EASYDMA_MAXCNT_SIZE: u32 = 20;

pub const POWER_BASE: usize = 0x4000_0000;
pub const CLOCK_BASE: usize = 0x4000_0000;
pub const NVMC_BASE: usize = 0x4001_E000;
pub const UARTE0_BASE: usize = 0x4000_2000;
pub const UARTE1_BASE: usize = 0x4002_8000;
pub const GPIO0_BASE: usize = 0x5000_0000;
pub const GPIO1_BASE: usize = 0x5000_0300;
pub const GPIOTE_BASE: usize = 0x4000_6000;
pub const TIMER0_BASE: usize = 0x4000_8000;
pub const RTC0_BASE: usize = 0x4000_B000;
pub const AAR_BASE: usize = 0x4001_0000;
pub const CCM_BASE: usize = 0x4001_0000;

#[repr(C)]
pub struct EasyDmaChannel {
    pub ptr: u32,
    pub maxcnt: u32,
    pub amount: u32,
}

#[repr(C)]
pub struct Gpio {
    pub padding: [u32; 321],
    pub out: u32,
    pub outset: u32,
    pub outclr: u32,
    pub in_: u32,
    pub dir: u32,
    pub dirset: u32,
    pub dirclr: u32,
    pub latch: u32,
    pub detect_mode: u32,
    pub _reserved0: [u32; 118],
    pub pin_cnf: [u32; 32],
}

#[repr(C)]
pub struct Gpiote {
    pub tasks_out: [u32; 8],
    pub tasks_set: [u32; 8],
    pub tasks_clr: [u32; 8],
    pub events_in: [u32; 8],
    pub events_port: u32,
    pub _reserved0: [u32; 7],
    pub ints_en: u32,
    pub ints_dis: u32,
    pub _reserved1: [u32; 1],
    pub config: [u32; 8],
}

#[repr(C)]
pub struct Uarte {
    pub tasks_start: u32,
    pub tasks_stop: u32,
    pub tasks_flush: u32,
    pub _reserved0: [u32; 61],
    pub events_cts: u32,
    pub events_ncts: u32,
    pub events_rxdrdy: u32,
    pub events_endrx: u32,
    pub events_txdrdy: u32,
    pub events_endtx: u32,
    pub events_error: u32,
    pub _reserved1: [u32; 5],
    pub shorts: u32,
    pub _reserved2: [u32; 2],
    pub inten_set: u32,
    pub inten_clr: u32,
    pub errorsrc: u32,
    pub _reserved3: [u32; 31],
    pub enable: u32,
    pub _reserved4: [u32; 1],
    pub psel_rts: u32,
    pub psel_txd: u32,
    pub psel_cts: u32,
    pub psel_rxd: u32,
    pub rxd: EasyDmaChannel,
    pub txd: EasyDmaChannel,
    pub baudrate: u32,
    pub config: u32,
}

#[repr(C)]
pub struct Timer {
    pub tasks_start: u32,
    pub tasks_stop: u32,
    pub tasks_count: u32,
    pub tasks_clear: u32,
    pub tasks_shutdown: u32,
    pub _reserved0: [u32; 11],
    pub events_compare: [u32; 6],
    pub _reserved1: [u32; 26],
    pub shorts: u32,
    pub _reserved2: [u32; 2],
    pub inten_set: u32,
    pub inten_clr: u32,
    pub mode: u32,
    pub bitmode: u32,
    pub _reserved3: [u32; 13],
    pub cc: [u32; 6],
}

#[repr(C)]
pub struct Power {
    pub tasks_constlat: u32,
    pub tasks_lofp: u32,
    pub tasks_off: u32,
    pub _reserved0: [u32; 5],
    pub events_pofwarn: u32,
    pub _reserved1: [u32; 2],
    pub events_sbe: u32,
    pub events_usbdetected: u32,
    pub events_usbremoved: u32,
    pub events_ready: u32,
    pub _reserved2: [u32; 1],
    pub shorts: u32,
    pub _reserved3: [u32; 2],
    pub inten_set: u32,
    pub inten_clr: u32,
    pub _reserved4: [u32; 13],
    pub resetreas: u32,
    pub _reserved5: [u32; 1],
    pub ram_power: [u32; 9],
    pub _reserved6: [u32; 5],
    pub vddh: u32,
}

pub unsafe fn gpio0() -> &'static mut Gpio {
    &mut *(GPIO0_BASE as *mut Gpio)
}

pub unsafe fn gpio1() -> &'static mut Gpio {
    &mut *(GPIO1_BASE as *mut Gpio)
}

pub unsafe fn uarte0() -> &'static mut Uarte {
    &mut *(UARTE0_BASE as *mut Uarte)
}

pub unsafe fn uarte1() -> &'static mut Uarte {
    &mut *(UARTE1_BASE as *mut Uarte)
}

pub unsafe fn gpiote() -> &'static mut Gpiote {
    &mut *(GPIOTE_BASE as *mut Gpiote)
}

pub unsafe fn timer0() -> &'static mut Timer {
    &mut *(TIMER0_BASE as *mut Timer)
}

pub unsafe fn power() -> &'static mut Power {
    &mut *(POWER_BASE as *mut Power)
}

pub unsafe fn nvmc() -> *mut u32 {
    NVMC_BASE as *mut u32
}

pub unsafe fn write_u32(addr: usize, value: u32) {
    ptr::write_volatile(addr as *mut u32, value);
}

pub unsafe fn read_u32(addr: usize) -> u32 {
    ptr::read_volatile(addr as *const u32)
}
