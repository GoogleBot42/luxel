//! Sensor input: PB sensor expansion board frames on UART0 RX — the
//! expansion header's RX0 pin, so the (open source) board plugs straight
//! in, PB-style. Frames are parsed by luxel_core::netin (shared with the
//! mirror and unit-tested) into shared::SENSOR_FRAME, which the render
//! task applies to the engine between frames. POST /api/sensors feeds the
//! same path over the network (server.rs).

use esp_hal::uart::UartRx;
use esp_hal::Async;
use luxel_core::netin::{parse_sensor_board, sb_find, SB_FRAME_LEN};

use crate::shared;

#[embassy_executor::task]
pub async fn uart_task(mut rx: UartRx<'static, Async>) -> ! {
    // heap, not future-locals: task futures are statics and statics eat
    // the main task stack (see main.rs heap comment)
    let mut buf: alloc::vec::Vec<u8> = alloc::vec::Vec::with_capacity(4 * SB_FRAME_LEN);
    let mut chunk = alloc::vec![0u8; 256];
    loop {
        match rx.read_async(&mut chunk).await {
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                // consume every complete frame; resync on garbage
                loop {
                    let Some(at) = sb_find(&buf) else {
                        // no header: keep a magic-sized tail in case one is split
                        if buf.len() > SB_FRAME_LEN {
                            let cut = buf.len() - SB_FRAME_LEN;
                            buf.drain(..cut);
                        }
                        break;
                    };
                    if buf.len() < at + SB_FRAME_LEN {
                        buf.drain(..at); // incomplete — wait for the rest
                        break;
                    }
                    match parse_sensor_board(&buf[at..at + SB_FRAME_LEN]) {
                        Some(s) => {
                            shared::set_sensor_frame(s);
                            buf.drain(..at + SB_FRAME_LEN);
                        }
                        None => {
                            buf.drain(..at + 1); // bad trailer — slide past
                        }
                    }
                }
            }
            Err(_) => embassy_time::Timer::after(embassy_time::Duration::from_millis(50)).await,
        }
    }
}
