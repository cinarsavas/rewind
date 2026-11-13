//! Live "how heavy is Rewind" numbers. libobs runs inside this process, so the process's own
//! CPU time includes capture and encoding, which is exactly what people want to see.

use std::mem::{size_of, zeroed};
use std::time::Instant;

use rewind_core::perf::{cpu_percent, smooth};
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

pub struct Sampler {
    last_cpu: u64,
    last_wall: Instant,
    cores: usize,
    pub cpu_pct: f32,
    pub mem_mb: u32,
}

fn ft(f: FILETIME) -> u64 {
    ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64
}

fn process_cpu_time() -> u64 {
    unsafe {
        let (mut c, mut e, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) = (zeroed(), zeroed(), zeroed(), zeroed());
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) != 0 {
            ft(k) + ft(u)
        } else {
            0
        }
    }
}

fn working_set_mb() -> u32 {
    unsafe {
        let mut pmc: PROCESS_MEMORY_COUNTERS = zeroed();
        pmc.cb = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) != 0 {
            (pmc.WorkingSetSize / (1024 * 1024)) as u32
        } else {
            0
        }
    }
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            last_cpu: process_cpu_time(),
            last_wall: Instant::now(),
            cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1),
            cpu_pct: 0.0,
            mem_mb: 0,
        }
    }

    /// Cheap to call every frame; only measures about every two seconds.
    pub fn sample_if_due(&mut self) {
        let wall = self.last_wall.elapsed();
        if wall.as_secs_f32() < 2.0 {
            return;
        }
        let cpu = process_cpu_time();
        let pct = cpu_percent(cpu.saturating_sub(self.last_cpu), (wall.as_nanos() / 100) as u64, self.cores);
        self.cpu_pct = smooth(self.cpu_pct, pct, 0.6);
        self.mem_mb = working_set_mb();
        self.last_cpu = cpu;
        self.last_wall = Instant::now();
    }
}
