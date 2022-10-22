#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::i2c::{Config, I2c};
use embassy_stm32::time::Hertz;
use embassy_time::Duration;
use {defmt_rtt as _, panic_probe as _};

const ADDRESS: u8 = 0x5F;
const WHOAMI: u8 = 0x0F;

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    info!("Hello world!");
    let p = embassy_stm32::init(Default::default());

    let mut config = Config::default();

    // Setting a timeout prevents i2c methods from spinning infinitely
    config.timeout = Some(Duration::from_millis(100));

    let mut i2c = I2c::new(p.I2C2, p.PB10, p.PB11, Hertz(100_000), config);

    let mut data = [0u8; 1];
    unwrap!(i2c.blocking_write_read(ADDRESS, &[WHOAMI], &mut data));
    info!("Whoami: {}", data[0]);
}
