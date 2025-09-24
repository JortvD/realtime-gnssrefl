use embassy_rp::pac::{
    self,
    clocks::{regs::ClkSysSelected, vals::ClkSysCtrlSrc, vals::ClkSysCtrlAuxsrc},
};
use embassy_rp::clocks;
use defmt::info;

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;

pub const CLK_HZ_HIGH: u32 = 150_000_000;
pub const CLK_HZ_LOW: u32 =   12_000_000;

enum ClkMsg {
    Request,
    Release,
}

pub struct ClkRequest {
    _private: ()
}

static CLK_CHANNEL: Channel<CriticalSectionRawMutex, ClkMsg, 8> = Channel::new();

fn sel_mask(src: ClkSysCtrlSrc) -> ClkSysSelected {
    // 1u32 to avoid i32 shifting
    ClkSysSelected(1u32 << src.to_bits())
}

/// Compute (int, frac) 16.16 divider from base->target.
/// Returns None if target is 0 or target > base (i.e. would need PLL change to go faster).
fn divider_16_16(base_hz: u32, target_hz: u32) -> Option<(u16, u16)> {
    if target_hz == 0 || target_hz > base_hz { return None; }
    let int = (base_hz / target_hz) as u32;
    // 16.16 fractional: div = int + frac/65536
    let rem = base_hz % target_hz;
    let frac = ((rem as u64) << 16) / (target_hz as u64);
    Some((int.min(u16::MAX as u32) as u16, frac.min(u16::MAX as u64) as u16))
}

/// Change clk_sys at runtime by adjusting the SYS divider on RP235x.
///
/// SAFETY: Touches clock control registers; call from a critical section if you have
/// very tight timing/peripheral constraints. Ensure target_hz <= PLL_SYS.
fn set_sysclk_hz_rp235x(target_hz: u32) -> Result<u32, &'static str> {
    let c = pac::CLOCKS;

    // Base sys comes from PLL_SYS on RP235x in Embassy's default config.
    let base = clocks::pll_sys_freq(); // e.g. 150_000_000 on Pico 2 (RP2350)
    let (int, frac) = divider_16_16(base, target_hz).ok_or("bad target_hz")?;

    // 1) Switch SYS to REF while we change the divider (glitch-free sequence).
    c.clk_sys_ctrl().write(|w| w.set_src(ClkSysCtrlSrc::CLK_REF));
    while c.clk_sys_selected().read() != sel_mask(ClkSysCtrlSrc::CLK_REF) {}

    // 2) Program the 16.16 divider.
    c.clk_sys_div().write(|w| { w.set_int(int); w.set_frac(frac); });

    // 3) Switch SYS back to PLL_SYS via AUX.
    c.clk_sys_ctrl().modify(|w| {
        w.set_auxsrc(ClkSysCtrlAuxsrc::CLKSRC_PLL_SYS);
        w.set_src(ClkSysCtrlSrc::CLKSRC_CLK_SYS_AUX);
    });
    while c.clk_sys_selected().read() != sel_mask(ClkSysCtrlSrc::CLKSRC_CLK_SYS_AUX) {}

    // Effective new SYS frequency = base / (int + frac/65536)
    let new_sys = ((base as u64) << 16) / (((int as u64) << 16) | (frac as u64));

    info!("CLK frequency: {:?}", new_sys);
    Ok(new_sys as u32)
}

impl Drop for ClkRequest {
    fn drop(&mut self) {
        let _ = CLK_CHANNEL.try_send(ClkMsg::Release);
    }
}

#[must_use = "High CLK is held while Clkrequest is alive. If the return value is not used, the request is not granted"]
pub fn clk_request() -> ClkRequest {
    let _ = CLK_CHANNEL.try_send(ClkMsg::Request);
    ClkRequest{ _private: () }
}

#[embassy_executor::task]
pub async fn clk_control(){
    let mut count = 0;
    set_sysclk_hz_rp235x(CLK_HZ_LOW).unwrap();
    loop {
        match CLK_CHANNEL.receive().await {
            ClkMsg::Request => {
                count+=1;
                if count == 1 {
                    set_sysclk_hz_rp235x(CLK_HZ_HIGH).unwrap();
                }
            }
            ClkMsg::Release => {
                count-=1;
                if count == 0 {
                    set_sysclk_hz_rp235x(CLK_HZ_LOW).unwrap();
                }
            }
        } 
    }
}