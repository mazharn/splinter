use super::cycles;

pub struct CycleCounter {
    total: u64,
    start_time: u64,
    run_count: u64,
    measurement_count: u64,
}

impl CycleCounter {

    /*
    pub fn new() -> CycleCounter {
        CycleCounter {
            total: 0,
            start_time: cycles::rdtsc(),
            run_count: 0,
            measurement_count: 0,
        }
    }
    */

    pub fn new(m_count: u64) -> CycleCounter {
        CycleCounter {
            total: 0,
            start_time: 0,
            run_count: 0,
            measurement_count: m_count,
        }
    }

    pub fn start(&mut self) {
        self.start_time = cycles::rdtsc();
    }

    pub fn stop(&mut self) -> u64 {
        let elapsed = cycles::rdtsc() - self.start_time;
        self.total += elapsed;
        self.run_count += 1;
        if self.run_count == self.measurement_count {
            info!("{}", cycles::to_seconds(self.total / self.run_count) * 1000000.);
            self.run_count = 0;
            self.total = 0;
        }
        elapsed
    }

    pub fn average(self) -> u64 {
        self.total / self.run_count
    }
}
