//! The peak memory that the harness reports, checked in a binary of its own: the count is the
//! whole program's, so the figure can be asserted only where nothing else runs. There is one test.

use std::time::Duration;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Parses, measure};

#[global_allocator]
static COUNTING: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;

fn bench(prepare: fn(&Input) -> Result<stetdoc_testkit::bench::Run, String>) -> Bench {
    Bench {
        name: "toy",
        limit: Limit::None,
        parses: Parses::Nothing,
        inputs: Inputs::Documents,
        prepare,
    }
}

#[test]
fn the_peak_counts_what_prepare_keeps_and_what_the_run_adds_and_nothing_freed_before() {
    const MIB: u64 = 1 << 20;
    // 2 MiB kept by prepare, 3 MiB of scratch inside the run that is there for a few milliseconds:
    // the peak is 5 MiB.
    let held = bench(|_| {
        let kept = vec![1u8; 2 << 20];
        Ok(Box::new(move || {
            let scratch = std::hint::black_box(vec![2u8; 3 << 20]);
            std::thread::sleep(Duration::from_millis(5));
            drop(scratch);
            Ok(Box::new(kept) as Kept)
        }))
    });
    let m = measure(&held, &Input::none()).unwrap();
    assert!(
        (5 * MIB..5 * MIB + (64 << 10)).contains(&m.peak),
        "{} bytes",
        m.peak
    );
    // The sampler took readings for as long as the run lasted: 5 ms or more, with the sleep.
    assert!(
        m.watched.readings > 0 && m.watched.over >= Duration::from_millis(5),
        "{:?}",
        m.watched
    );

    // 4 MiB that prepare allocates and frees before the run starts are not the run's.
    let freed = bench(|_| {
        drop(std::hint::black_box(vec![3u8; 4 << 20]));
        Ok(Box::new(|| Ok(Box::new(vec![0u8; 1 << 10]) as Kept)))
    });
    let m = measure(&freed, &Input::none()).unwrap();
    assert!(m.peak < 64 << 10, "{} bytes", m.peak);

    // What a run returns is dropped after the clock stops, and the peak is read before that.
    let returned = bench(|_| Ok(Box::new(|| Ok(Box::new(vec![4u8; 6 << 20]) as Kept))));
    let m = measure(&returned, &Input::none()).unwrap();
    assert!(
        (6 * MIB..6 * MIB + (64 << 10)).contains(&m.peak),
        "{} bytes",
        m.peak
    );

    // A vector that grows by pushes is counted by what it holds, not by what the copies add up
    // to: the largest block and a little more, and not the last two blocks together (12 MiB).
    let grown = bench(|_| {
        Ok(Box::new(|| {
            let mut v: Vec<u8> = Vec::new();
            for i in 0..(8u32 << 20) {
                v.push(i as u8);
            }
            Ok(Box::new(v) as Kept)
        }))
    });
    let m = measure(&grown, &Input::none()).unwrap();
    assert!(
        (8 * MIB..8 * MIB + (64 << 10)).contains(&m.peak),
        "{} bytes",
        m.peak
    );
}
