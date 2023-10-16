#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::mem;

use alloc_cortex_m::CortexMHeap;
use cortex_m_rt::entry;
use defmt::{info, unwrap};
use embassy_executor::{Executor, Spawner};
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

async fn run1() {
    for _ in 0..5 {
        info!("BIG INFREQUENT TICK");
        Timer::after(Duration::from_millis(2000)).await;
    }
}

async fn run2() {
    for _ in 0..5 {
        info!("tick");
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();
const HEAP_SIZE: usize = 16 * 1024;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Hello World!");

    // Initialize allocator
    unsafe {
        ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE);
    }

    let _p = embassy_stm32::init(Default::default());

    spawner.spawn_alloc(run1).unwrap();
    spawner.spawn_alloc(run2).unwrap();

    loop {
        info!("Allocator used: {}, free: {}", ALLOCATOR.used(), ALLOCATOR.free());
        Timer::after(Duration::from_millis(1000)).await;

        spawner.spawn_alloc(run2).unwrap();
    }
}
