//! Benchmarks for the timer subsystem
//!
//! Run with: cargo bench -p sim-core

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sim_core::timers::{create_platform_timer, IdleManager, ThrottleManager, ThrottleType, TimerManager};
use std::hint::black_box;

fn bench_platform_timer_get_msec(c: &mut Criterion) {
    let platform = create_platform_timer();

    c.bench_function("platform_get_msec", |b| {
        b.iter(|| black_box(platform.get_msec()));
    });
}

fn bench_platform_timer_get_time_spec(c: &mut Criterion) {
    let platform = create_platform_timer();

    c.bench_function("platform_get_time_spec", |b| {
        b.iter(|| black_box(platform.get_time_spec()));
    });
}

fn bench_timer_manager_creation(c: &mut Criterion) {
    c.bench_function("timer_manager_new", |b| {
        b.iter(|| {
            let platform = create_platform_timer();
            black_box(TimerManager::new(platform))
        });
    });
}

fn bench_timer_initialization(c: &mut Criterion) {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    c.bench_function("init_timer", |b| {
        b.iter(|| black_box(timer_mgr.init_timer(0, 60.0).unwrap()));
    });
}

fn bench_get_rtc(c: &mut Criterion) {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);
    timer_mgr.init_timer(0, 60.0).unwrap();

    c.bench_function("get_rtc", |b| {
        b.iter(|| black_box(timer_mgr.get_rtc(0).unwrap()));
    });
}

fn bench_instructions_tracking(c: &mut Criterion) {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    c.bench_function("add_instructions", |b| {
        b.iter(|| {
            timer_mgr.add_instructions(black_box(1000));
        });
    });
}

fn bench_idle_check(c: &mut Criterion) {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);
    timer_mgr.init_timer(0, 60.0).unwrap();
    timer_mgr.set_enabled(true);

    c.bench_function("idle_check_disabled", |b| {
        timer_mgr.set_enabled(false);
        b.iter(|| black_box(timer_mgr.try_idle(0, 1000)));
    });

    c.bench_function("idle_check_enabled_unstable", |b| {
        timer_mgr.set_enabled(true);
        b.iter(|| black_box(timer_mgr.try_idle(0, 1000)));
    });
}

fn bench_throttle_validation(c: &mut Criterion) {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);

    c.bench_function("set_throttle_none", |b| {
        b.iter(|| black_box(timer_mgr.set_throttle(ThrottleType::None).unwrap()));
    });

    c.bench_function("set_throttle_mcps", |b| {
        b.iter(|| black_box(timer_mgr.set_throttle(ThrottleType::MegaCyclesPerSec(1)).unwrap()));
    });

    c.bench_function("set_throttle_specific", |b| {
        b.iter(|| {
            black_box(
                timer_mgr
                    .set_throttle(ThrottleType::Specific {
                        instructions: 1000,
                        delay_ms: 10,
                    })
                    .unwrap(),
            )
        });
    });
}

fn bench_throttle_schedule(c: &mut Criterion) {
    let platform = create_platform_timer();
    let mut timer_mgr = TimerManager::new(platform);
    timer_mgr.set_throttle(ThrottleType::None).unwrap();

    c.bench_function("schedule_throttle_none", |b| {
        b.iter(|| {
            timer_mgr.schedule_throttle(black_box(1000));
        });
    });
}

fn bench_multiple_timers(c: &mut Criterion) {
    let mut group = c.benchmark_group("multiple_timers");

    for timer_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(timer_count),
            timer_count,
            |b, &count| {
                let platform = create_platform_timer();
                let mut timer_mgr = TimerManager::new(platform);

                // Initialize timers
                for i in 0..count {
                    timer_mgr.init_timer(i, 60.0 * (i + 1) as f64).unwrap();
                }

                b.iter(|| {
                    for i in 0..count {
                        black_box(timer_mgr.get_rtc(i).unwrap());
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_platform_sleep_overhead(c: &mut Criterion) {
    let platform = create_platform_timer();
    let min_sleep = platform.get_sleep_min_ms();

    c.bench_function("sleep_min_overhead", |b| {
        b.iter(|| black_box(platform.sleep_ms(0)));
    });

    c.bench_function("sleep_actual_min", |b| {
        b.iter(|| black_box(platform.sleep_ms(min_sleep)));
    });
}

fn bench_clock_characteristics(c: &mut Criterion) {
    let platform = create_platform_timer();
    let timer_mgr = TimerManager::new(platform);

    c.bench_function("clock_characteristics", |b| {
        b.iter(|| black_box(timer_mgr.clock_characteristics()));
    });
}

criterion_group!(
    platform_benches,
    bench_platform_timer_get_msec,
    bench_platform_timer_get_time_spec,
    bench_platform_sleep_overhead,
);

criterion_group!(
    manager_benches,
    bench_timer_manager_creation,
    bench_timer_initialization,
    bench_get_rtc,
    bench_instructions_tracking,
    bench_clock_characteristics,
);

criterion_group!(idle_benches, bench_idle_check,);

criterion_group!(
    throttle_benches,
    bench_throttle_validation,
    bench_throttle_schedule,
);

criterion_group!(scaling_benches, bench_multiple_timers,);

criterion_main!(
    platform_benches,
    manager_benches,
    idle_benches,
    throttle_benches,
    scaling_benches,
);
